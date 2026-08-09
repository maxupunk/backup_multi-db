-- Fixture de MariaDB para a suíte de contrato.
--
-- Quase idêntico ao de MySQL, com duas diferenças deliberadas:
--   * `JSON` no MariaDB é apelido de LONGTEXT com CHECK json_valid() — o dump
--     sai diferente do MySQL, e o pipeline de restore precisa aguentar os dois.
--   * `utf8mb4_uca1400_ai_ci` é a collation default do MariaDB 11.4; deixar
--     explícito evita que o teste dependa da default da imagem.

CREATE DATABASE IF NOT EXISTS fixture_secondary;

USE app_fixture;

CREATE TABLE customers (
  id          INT AUTO_INCREMENT PRIMARY KEY,
  name        VARCHAR(120) NOT NULL,
  email       VARCHAR(254) NOT NULL UNIQUE,
  balance     DECIMAL(12, 2) NOT NULL DEFAULT 0,
  is_active   TINYINT(1) NOT NULL DEFAULT 1,
  notes       TEXT NULL,
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE orders (
  id           INT AUTO_INCREMENT PRIMARY KEY,
  customer_id  INT NOT NULL,
  total        DECIMAL(12, 2) NOT NULL,
  status       ENUM('pending', 'paid', 'cancelled') NOT NULL DEFAULT 'pending',
  payload      JSON NULL,
  created_at   DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_orders_customer FOREIGN KEY (customer_id) REFERENCES customers (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_orders_status ON orders (status);

CREATE VIEW active_customers AS
  SELECT id, name, email FROM customers WHERE is_active = 1;

INSERT INTO customers (name, email, balance, is_active, notes) VALUES
  ('Alice Souza',   'alice@example.test',   1500.50, 1, 'cliente com acento: ção, ãe, ü'),
  ('Bruno Lima',    'bruno@example.test',      0.00, 1, NULL),
  ('Carla Prado',   'carla@example.test',   -250.75, 0, 'saldo negativo'),
  ('Diego Martins', 'diego@example.test',  99999.99, 1, 'aspas '' e "duplas" e barra \\');

INSERT INTO orders (customer_id, total, status, payload) VALUES
  (1,  250.00, 'paid',      '{"items": 3, "coupon": null}'),
  (1,   99.90, 'pending',   '{"items": 1}'),
  (2, 1200.00, 'cancelled', NULL),
  (4,    0.01, 'paid',      '{"items": 0, "note": "valor mínimo"}');

USE fixture_secondary;

CREATE TABLE audit_sample (
  id      INT AUTO_INCREMENT PRIMARY KEY,
  message VARCHAR(255) NOT NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

INSERT INTO audit_sample (message) VALUES ('registro do database secundário');

GRANT ALL PRIVILEGES ON fixture_secondary.* TO 'tester'@'%';
FLUSH PRIVILEGES;
