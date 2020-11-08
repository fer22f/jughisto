CREATE TABLE problem (
  id text primary key not null,
  name text not null,
  memory_limit_bytes integer not null,
  time_limit_ms integer not null,
  checker_path text not null,
  checker_language text not null,
  validator_path text not null,
  validator_language text not null,
  main_solution_path text not null,
  main_solution_language text not null,
  test_count integer not null,
  test_pattern text not null,
  status text not null,
  creation_user_id integer references user(id) not null,
  creation_instant timestamp not null
)
