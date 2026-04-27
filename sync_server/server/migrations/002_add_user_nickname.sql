ALTER TABLE users ADD COLUMN nickname TEXT NOT NULL DEFAULT '';

UPDATE users
SET nickname = email
WHERE nickname IS NULL OR nickname = '';
