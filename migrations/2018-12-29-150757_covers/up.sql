create table covers (
  id serial primary key,
  issue integer not null references issues(id),
  image bytea not null,
  fetch_time timestamp not null
)
