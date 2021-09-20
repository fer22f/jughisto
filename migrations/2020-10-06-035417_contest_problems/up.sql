CREATE TABLE contest_problems (
  id serial primary key not null,
  label text not null,
  contest_id integer references contest(id) not null,
  problem_id text references problem(id) not null
)
