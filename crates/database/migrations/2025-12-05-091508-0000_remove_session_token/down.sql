-- This file should undo anything in `up.sql`
ALTER TABLE account
ADD COLUMN session_token VARCHAR(1024);