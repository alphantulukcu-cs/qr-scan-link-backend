ALTER TABLE scan_invites
    ADD COLUMN IF NOT EXISTS session_metadata JSONB;

ALTER TABLE scan_checks
    ADD COLUMN IF NOT EXISTS metadata JSONB;
