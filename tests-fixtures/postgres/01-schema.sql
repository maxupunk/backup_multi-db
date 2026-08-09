-- Fixture de PostgreSQL para a suíte de contrato.
--
-- Espelha o fixture de MySQL em conteúdo, mas usa os tipos idiomáticos do PG
-- (SERIAL, JSONB, tipo enumerado próprio) — o pipeline de dump/restore precisa
-- lidar com ambos os dialetos.

\connect app_fixture

CREATE TYPE order_status AS ENUM ('pending', 'paid', 'cancelled');

CREATE TABLE customers (
  id         SERIAL PRIMARY KEY,
  name       VARCHAR(120) NOT NULL,
  email      VARCHAR(254) NOT NULL UNIQUE,
  balance    NUMERIC(12, 2) NOT NULL DEFAULT 0,
  is_active  BOOLEAN NOT NULL DEFAULT TRUE,
  notes      TEXT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE orders (
  id          SERIAL PRIMARY KEY,
  customer_id INTEGER NOT NULL REFERENCES customers (id) ON DELETE CASCADE,
  total       NUMERIC(12, 2) NOT NULL,
  status      order_status NOT NULL DEFAULT 'pending',
  payload     JSONB NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_orders_status ON orders (status);

CREATE VIEW active_customers AS
  SELECT id, name, email FROM customers WHERE is_active;

INSERT INTO customers (name, email, balance, is_active, notes) VALUES
  ('Alice Souza',   'alice@example.test',   1500.50, TRUE,  'cliente com acento: ção, ãe, ü'),
  ('Bruno Lima',    'bruno@example.test',      0.00, TRUE,  NULL),
  ('Carla Prado',   'carla@example.test',   -250.75, FALSE, 'saldo negativo'),
  ('Diego Martins', 'diego@example.test',  99999.99, TRUE,  'aspas '' e "duplas" e barra \');

INSERT INTO orders (customer_id, total, status, payload) VALUES
  (1,  250.00, 'paid',      '{"items": 3, "coupon": null}'),
  (1,   99.90, 'pending',   '{"items": 1}'),
  (2, 1200.00, 'cancelled', NULL),
  (4,    0.01, 'paid',      '{"items": 0, "note": "valor mínimo"}');

-- Segundo database, para `discover-databases` e seleção por database.
CREATE DATABASE fixture_secondary OWNER tester;

\connect fixture_secondary

CREATE TABLE audit_sample (
  id      SERIAL PRIMARY KEY,
  message VARCHAR(255) NOT NULL
);

INSERT INTO audit_sample (message) VALUES ('registro do database secundário');
