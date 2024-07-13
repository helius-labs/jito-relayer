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
    REGION 'us-east-2'
);