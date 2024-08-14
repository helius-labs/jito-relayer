use duckdb::{params, Connection};

use jito_transaction_relayer::db_service::TransactionRow;

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn Error>> {
    let conn = Connection::open_in_memory().unwrap();
    let ddl = "\
install aws;
install httpfs;
load aws;
load httpfs;
CREATE TABLE transactions (
    ts BIGINT NOT NULL,
    priority UBIGINT NOT NULL,
    cu_limit UBIGINT NOT NULL,
    hash VARCHAR NOT NULL,
    payer VARCHAR NOT NULL,
    source_ip UINTEGER NOT NULL,
    remote_pubkey VARCHAR NOT NULL,
    num_sigs UTINYINT NOT NULL,
);
CREATE SECRET (
    TYPE S3,
    PROVIDER CREDENTIAL_CHAIN,
    CHAIN 'env;config',
    REGION 'us-east-2'
);
    ";

    let sync = include_str!("./sql/copy_to_s3.sql");
    let sync_replaced = str::replace(sync, "XXXXXX", "s3://helius-traffic-scoring/auth-tester/");
    let epoch_millis = || {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    };
    // get random number generator
    conn.execute_batch(ddl).unwrap();

    let mut appender = conn.appender("transactions").unwrap();
    let row = TransactionRow {
        ts: epoch_millis(),
        priority: 1,
        cu_limit: 2,
        hash: "hash".to_string(),
        payer: "payer".to_string(),
        source: 3,
        remote_pubkey: "remote_pubkey".to_string(),
        num_sigs: 4,
    };

    appender
        .append_row(params![
            row.ts,
            row.priority,
            row.cu_limit,
            row.hash,
            row.payer,
            row.source,
            row.remote_pubkey,
            row.num_sigs
        ])
        .unwrap();
    appender.flush().unwrap();
    drop(appender);
    // conn.execute(
    //     &format!("INSERT INTO transactions VALUES ({}, 2, 3, 'hash', 'payer', 4);", epoch_millis()),
    //     params![]
    // ).unwrap();
    conn.execute_batch(&sync_replaced).unwrap();

    Ok(())
}
