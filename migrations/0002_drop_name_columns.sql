ALTER TABLE scan_invites
    DROP COLUMN IF EXISTS employee_first_name,
    DROP COLUMN IF EXISTS employee_last_name,
    DROP COLUMN IF EXISTS employee_national_id,
    DROP COLUMN IF EXISTS employee_email,
    DROP COLUMN IF EXISTS customer_first_name,
    DROP COLUMN IF EXISTS customer_last_name;
