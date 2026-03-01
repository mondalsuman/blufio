-- Add intended_model column for model routing tracking.
-- Records what model the classifier intended before budget downgrades.
-- NULL when routing is disabled or no downgrade occurred.

ALTER TABLE cost_ledger ADD COLUMN intended_model TEXT;
