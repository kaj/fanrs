-- Create the titles table

create table titles (
  id serial primary key,
  title varchar not null,
  slug varchar not null
);

create unique index titles_title on titles (title);
create unique index titles_slug on titles (slug);
