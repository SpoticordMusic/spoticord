-- Trigger functions

CREATE OR REPLACE FUNCTION update_last_updated_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.last_updated = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Tables

CREATE TABLE "user" (
    id VARCHAR PRIMARY KEY,
    device_name VARCHAR(32) NOT NULL DEFAULT 'Spoticord'
);

CREATE TABLE "link_request" (
    token TEXT PRIMARY KEY,
    user_id TEXT UNIQUE NOT NULL,
    expires TIMESTAMP NOT NULL,

    CONSTRAINT fk_user_id FOREIGN KEY (user_id) REFERENCES "user" (id)
);

CREATE TABLE "account" (
    user_id VARCHAR PRIMARY KEY,
    username VARCHAR(64) NOT NULL,
    access_token VARCHAR(1024) NOT NULL,
    refresh_token VARCHAR(1024) NOT NULL,
    session_token VARCHAR(1024),
    expires TIMESTAMP NOT NULL,
    last_updated TIMESTAMP NOT NULL DEFAULT NOW(),

    CONSTRAINT fk_account_user_id FOREIGN KEY (user_id) REFERENCES "user" (id)
);

-- Triggers

CREATE TRIGGER update_last_updated_column
BEFORE UPDATE ON "account"
FOR EACH ROW
EXECUTE FUNCTION update_last_updated_column();