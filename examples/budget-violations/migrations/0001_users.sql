DROP TABLE IF EXISTS users;
CREATE TABLE users (id INTEGER PRIMARY KEY, email TEXT NOT NULL, name TEXT NOT NULL);
WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM seq WHERE n<2000)
INSERT INTO users SELECT n, 'user' || n || '@example.test', 'Fixture ' || n FROM seq;
