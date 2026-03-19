-- Add migration script here
ALTER TABLE subscription_tokens ADD COLUMN created_at TIMESTAMPTZ NULL;