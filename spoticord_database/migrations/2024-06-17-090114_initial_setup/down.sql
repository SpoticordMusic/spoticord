-- Tables

DROP TABLE "account";
DROP TABLE "link_request";
DROP TABLE "user";

-- Trigger functions

DROP FUNCTION IF EXISTS delete_inactive_accounts();
DROP FUNCTION IF EXISTS update_last_updated_column();