CREATE TABLE user (
  id integer primary key autoincrement not null,
  name text not null,
  hashed_password text not null,
  is_admin boolean not null
)
