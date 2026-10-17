-- Item ids that failed during a scan, as a JSON array, for "retry failed".
ALTER TABLE scan_runs ADD COLUMN failed_items TEXT;
