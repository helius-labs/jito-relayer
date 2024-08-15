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
    num_sigs UTINYINT NOT NULL
);
CREATE SECRET aws(
    TYPE S3,
    PROVIDER CREDENTIAL_CHAIN,
    REGION 'us-east-2'
);