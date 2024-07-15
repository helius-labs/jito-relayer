use duckdb::{Connection, params};

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};
use openssl::rand;

fn main() -> Result<(), Box<dyn Error>>{

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
    let epoch_millis =  || SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    // get random number generator
    conn.execute_batch(ddl).unwrap();
    conn.execute(
        &format!("INSERT INTO transactions VALUES ({}, 2, 3, 'hash', 'payer', 4);", epoch_millis()),
        params![]
    ).unwrap();
    conn.execute(&sync_replaced, params![]).unwrap();

    Ok(())
}