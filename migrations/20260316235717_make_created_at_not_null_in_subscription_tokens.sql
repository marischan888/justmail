-- Add migration script here
BEGIN;
    UPDATE subscription_tokens
        SET created_at = now()
        WHERE created_at ISNULL;
    ALTER TABLE subscription_tokens ALTER COLUMN created_at SET NOT NULL;
COMMIT;