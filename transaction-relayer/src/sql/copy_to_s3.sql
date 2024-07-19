BEGIN TRANSACTION;
COPY (
    SELECT
      *,
      make_timestamp(ts * 1000000) as sts,
      date_trunc('DAY',sts) as date_id,
    FROM transactions
) TO 'XXXXXX'
(FORMAT PARQUET, PARTITION_BY (date_id), OVERWRITE_OR_IGNORE, FILENAME_PATTERN "txn_scores_{uuid}");
DELETE FROM transactions;
COMMIT;