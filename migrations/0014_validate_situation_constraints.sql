-- Validate separately from the data rewrite so constraint scanning does not
-- extend the high-write migration transaction in 0013.

ALTER TABLE events VALIDATE CONSTRAINT events_strength_check;
ALTER TABLE events VALIDATE CONSTRAINT events_situation_code_check;
