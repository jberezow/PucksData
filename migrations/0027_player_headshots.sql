-- Preserve the NHL-provided player headshot URL alongside landing-page identity
-- metadata. The source may omit a headshot, especially for historical players.

ALTER TABLE players
    ADD COLUMN headshot_url TEXT;

COMMENT ON COLUMN players.headshot_url IS
    'Optional NHL-provided player headshot URL from the player landing payload.';
