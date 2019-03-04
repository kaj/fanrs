-- Another magazine (etc) where a previous publication occurs
create table other_mags (
  id serial primary key,
  name varchar not null,
  issue smallint,
  i_of smallint,
  year smallint
);

create unique index other_mags_all on other_mags (name, issue, i_of, year);

alter table episodes add column orig_mag integer references other_mags(id);
