use duckdb::{params, Connection, Error};
use jito_core::immutable_deserialized_packet::ImmutableDeserializedPacket;
use log::{error, info};
use std::net::SocketAddr::V4;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::select;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::timeout;

const DDL: &str = include_str!("./sql/ddl.sql");
const SYNC_STMT: &str = include_str!("./sql/copy_to_s3.sql");
const LOCATION_TOKEN: &str = "XXXXXX";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransactionRow {
    pub ts: u64,
    pub priority: u64,
    pub cu_limit: u64,
    pub hash: String,
    pub payer: String,
    pub source: u32,
}

impl TransactionRow {
    pub fn new(
        ts: u64,
        priority: u64,
        cu_limit: u64,
        hash: String,
        payer: String,
        source: u32,
    ) -> Self {
        Self {
            ts,
            priority,
            cu_limit,
            hash,
            payer,
            source,
        }
    }
}

impl From<(u64, ImmutableDeserializedPacket)> for TransactionRow {
    fn from(bundle: (u64, ImmutableDeserializedPacket)) -> TransactionRow {
        let (ts, packet) = bundle;

        let priority = packet.priority();
        let cu_limit = packet.compute_unit_limit();
        let hash = packet.message_hash().to_string();
        let payer = packet
            .transaction()
            .get_message()
            .message
            .static_account_keys()
            .get(0)
            .unwrap()
            .to_string();

        let numeric_ip: u32 = if let V4(addr) = packet.original_packet().meta().socket_addr() {
            (*addr.ip()).into()
        } else {
            0
        };

        Self::new(ts, priority, cu_limit, hash, payer, numeric_ip)
    }
}

pub struct DBSink {
    exit_flag: Arc<AtomicBool>,
    receiver: UnboundedReceiver<TransactionRow>,
    sync_query: String,
    conn: Arc<Mutex<Connection>>,
}
// unsafe impl Send for DBSink {}
impl DBSink {
    pub fn new(
        exit_flag: Arc<AtomicBool>,
        receiver: UnboundedReceiver<TransactionRow>,
        sync_location: String,
        conn: Arc<Mutex<Connection>>,
    ) -> Self {
        {
            let guard = conn.lock().unwrap();
            guard.execute_batch(DDL).unwrap();
        }
        Self {
            exit_flag,
            receiver,
            sync_query: str::replace(SYNC_STMT, LOCATION_TOKEN, &sync_location),
            conn: conn.clone(),
        }
    }

    pub async fn run(&mut self) {
        let mut flush_interval = tokio::time::interval(std::time::Duration::from_secs(1));
        let mut s3_sync_interval = tokio::time::interval(std::time::Duration::from_secs(900));
        let mut buffer: Vec<TransactionRow> = Vec::with_capacity(4096);
        while !self.exit_flag.load(std::sync::atomic::Ordering::Relaxed) {
            select! {
                 _ = flush_interval.tick() => {
                    if let Ok(rows) = timeout(Duration::from_millis(10), self.receiver.recv_many(&mut buffer, 4096)).await {
                       if rows > 0{
                            let g = self.conn.lock().unwrap();
                            let mut appender = g.appender("transactions").unwrap();
                            let mut ct = 0;
                            for row in buffer.iter() {
                                if let Err(e) = appender.append_row(params![
                                    row.ts,
                                    row.priority,
                                    row.cu_limit,
                                    row.hash,
                                    row.payer,
                                    row.source
                                ]) {
                                    error!("Error appending row: {e}");
                                } else {
                                    ct += 1;
                                }
                            }
                            match appender.flush() {

                                Ok(_) => {
                                    info!("Flushed {ct} rows to DB");
                                }
                                Err(e) => {
                                    error!("Error flushing appender: {e}");
                                }

                            }
                            drop(appender);
                            buffer.clear();
                        }
                    }
                }
                _ = s3_sync_interval.tick() => {
                    self.sync();
                }
            }
        }
        self.sync();
    }

    fn sync(&self) {
        let guard = self.conn.lock().unwrap();
        info!("Executing sync query: {}", self.sync_query);
        match guard.execute_batch(&self.sync_query) {
            Ok(()) => {}
            Err(e) => {
                error!("Error executing S3 sync: {e}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use duckdb::Connection;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::mpsc::unbounded_channel;

    #[test]
    fn test_transaction_row() {
        let row = TransactionRow::new(1, 2, 3, "hash".to_string(), "payer".to_string(), 4);
        assert_eq!(row.ts, 1);
        assert_eq!(row.priority, 2);
        assert_eq!(row.cu_limit, 3);
        assert_eq!(row.hash, "hash");
        assert_eq!(row.payer, "payer");
        assert_eq!(row.source, 4);
    }

    #[tokio::test]
    async fn test_db_sink() -> Result<(), Error> {
        let conn = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let (tx, rx) = unbounded_channel();
        let exit_flag = Arc::new(AtomicBool::new(false));
        let mut sink = DBSink::new(exit_flag.clone(), rx, "s3://test".to_string(), conn.clone());
        let handle = tokio::spawn(async move { sink.run().await });
        let initial_row = TransactionRow::new(1, 2, 3, "hash".to_string(), "payer".to_string(), 4);
        tx.send(initial_row.clone()).unwrap();

        tokio::time::sleep(std::time::Duration::from_secs(6)).await;
        let guard = conn.lock().unwrap();

        let mut statement = guard.prepare("SELECT * FROM transactions")?;
        let round_trip_row = statement
            .query_row(params![], |x| {
                Ok(TransactionRow {
                    ts: x.get(0)?,
                    priority: x.get(1)?,
                    cu_limit: x.get(2)?,
                    hash: x.get(3)?,
                    payer: x.get(4)?,
                    source: x.get(5)?,
                })
            })
            .unwrap();

        assert_eq!(initial_row, round_trip_row);
        drop(tx);
        exit_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        handle.await.unwrap();
        Ok(())
    }
}
