CREATE TABLE user (
  id integer primary key autoincrement not null,
  name text not null,
  hashed_password text not null,
  is_admin boolean not null,
  creation_user_id integer references user(id) null,
  creation_instant timestamp not null
)
