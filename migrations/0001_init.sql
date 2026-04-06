CREATE TABLE IF NOT EXISTS scan_invites (
    invite_id UUID PRIMARY KEY,
    one_time_token_hash TEXT NOT NULL UNIQUE,
    session_token_hash TEXT,
    employee_first_name TEXT NOT NULL,
    employee_last_name TEXT NOT NULL,
    employee_national_id TEXT NOT NULL,
    employee_email TEXT NOT NULL,
    customer_first_name TEXT NOT NULL,
    customer_last_name TEXT NOT NULL,
    customer_national_id TEXT NOT NULL,
    customer_email TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('pending', 'claimed', 'submitted', 'expired')),
    claim_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    claimed_at TIMESTAMPTZ,
    submitted_at TIMESTAMPTZ,
    batch_image_data_url TEXT
);

CREATE INDEX IF NOT EXISTS idx_scan_invites_created_at ON scan_invites (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_scan_invites_status ON scan_invites (status);

CREATE TABLE IF NOT EXISTS scan_checks (
    id BIGSERIAL PRIMARY KEY,
    invite_id UUID NOT NULL REFERENCES scan_invites(invite_id) ON DELETE CASCADE,
    sequence_no INTEGER NOT NULL CHECK (sequence_no > 0),
    qr_value TEXT NOT NULL,
    image_data_url TEXT NOT NULL,
    captured_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (invite_id, sequence_no)
);

CREATE INDEX IF NOT EXISTS idx_scan_checks_invite ON scan_checks (invite_id, sequence_no);
