BEGIN TRANSACTION;
COPY (
    SELECT
      *,
      make_timestamp(ts * 1000000) as sts,
      year(sts) as year,
      month(sts) as month,
      day(sts) as day,
      hour(sts) as hour
    FROM transactions
) TO 'XXXXXX'
(FORMAT PARQUET, PARTITION_BY (year, month, day, hour), OVERWRITE_OR_IGNORE, FILENAME_PATTERN "txn_scores_{uuid}");
DELETE FROM transactions;
COMMIT;