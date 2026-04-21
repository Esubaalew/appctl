-- Create anon role for PostgREST
DO $$ BEGIN
  CREATE ROLE anon NOLOGIN;
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS items (
  id   SERIAL PRIMARY KEY,
  name TEXT NOT NULL,
  done BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE IF NOT EXISTS notes (
  id      SERIAL PRIMARY KEY,
  content TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Grant read + write to anon for demo purposes
GRANT USAGE ON SCHEMA public TO anon;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO anon;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO anon;
