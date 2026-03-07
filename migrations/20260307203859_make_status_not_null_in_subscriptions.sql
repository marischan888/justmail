-- Add migration script here
BEGIN;
    -- Backfill 'status' for historical entries
    UPDATE subscriptions
        SET status = 'confirmed'
        WHERE status ISNULL;
    -- Make 'status' mandatory
    ALTER TABLE subscriptions ALTER COLUMN status SET NOT NULL;
COMMIT;