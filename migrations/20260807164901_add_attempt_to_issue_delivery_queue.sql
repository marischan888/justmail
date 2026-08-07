-- Add migration script here
ALTER TABLE issue_delivery_queue ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
