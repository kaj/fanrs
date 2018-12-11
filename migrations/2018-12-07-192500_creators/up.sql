-- Your SQL goes here

create table creators (
  id serial primary key,
  name varchar(200) unique not null,
  slug varchar(200) unique not null
);

-- create unique index creators_name on creators(name);
-- create unique index creators_slug on creators(slug);

create table creator_aliases (
  id serial primary key,
  creator_id integer not null references creators(id),
  name varchar(200) unique not null
);

-- FIXME rename to covers_by
create table cover_by (
  id serial primary key,
  issue_id integer not null references issues(id),
  by_id integer not null references creator_aliases(id)
  -- Should there be a role here as well?  ink / pencils / etc?
);

create unique index cover_by_natural on cover_by(issue_id, by_id);

-- FIXME rename to episodes_by
create table creativeparts (
  id serial primary key,
  episode_id integer not null references episodes(id),
  by_id integer not null references creator_aliases(id),
  role varchar(10) not null
);

create unique index creativeparts_natural on creativeparts(episode_id, by_id, role);

create table articles_by (
  id serial primary key,
  article_id integer not null references articles(id),
  by_id integer not null references creator_aliases(id),
  role varchar(10) not null
);

create unique index articles_by_natural on articles_by(article_id, by_id, role);

-- functions only used in this migration
create function insert_creator(slugp varchar(200), namep varchar(200))
returns void
as 'insert into creators (name, slug) values (namep, slugp);insert into creator_aliases (creator_id, name) select id, namep from creators where slug=slugp;'
language sql;

create function insert_alias(slugp varchar(200), namep varchar(200))
returns void
as 'insert into creator_aliases (creator_id, name) select id, namep from creators where slug=slugp;'
language sql;


select insert_creator('falk', 'Lee Falk');
select insert_alias  ('falk', 'Falk');

select insert_creator('mccoy', 'Wilson McCoy');
select insert_alias  ('mccoy', 'Wilson Mc Coy');
select insert_alias  ('mccoy', 'McCoy');

select insert_creator('barry', 'Sy Barry');
select insert_alias  ('barry', 'Barry');

select insert_creator('abel-romero', 'Abel Romero');
select insert_alias  ('abel-romero', 'A. Romero');
select insert_creator('al-gordon', 'Al Gordon');
select insert_alias  ('al-gordon', 'Alan Gordon');
select insert_alias  ('al-gordon', 'Gordon');
select insert_creator('alex-saviuk', 'Alex Saviuk');
select insert_alias  ('alex-saviuk', 'Saviuk');
select insert_creator('wox', 'Alf Woxnerud');
select insert_alias  ('wox', 'Wox');
select insert_creator('alfredo-p-alcala', 'Alfredo P. Alcala');
select insert_alias  ('alfredo-p-alcala', 'Alfred P. Alcala');
select insert_alias  ('alfredo-p-alcala', 'Red Alcala');
select insert_creator('alfredo-castelli', 'Alfredo Castelli');
select insert_alias  ('alfredo-castelli', 'Castelli');
select insert_creator('anders-eklund', 'Anders Eklund');
select insert_alias  ('anders-eklund', 'A. Eklund');
select insert_creator('andre-franquin', 'André Franquin');
select insert_alias  ('andre-franquin', 'Franquin');
select insert_creator('anette-salmelin', 'Anette Salmelin');
select insert_alias  ('anette-salmelin', 'A. Salmelin');
select insert_creator('ann-schwenke', 'Ann Schwenke');
select insert_alias  ('ann-schwenke', 'A. Schwenke');
select insert_creator('anniqa-tjernlund', 'Anniqa Tjernlund');
select insert_alias  ('anniqa-tjernlund', 'A. Tjernlund');
select insert_creator('bengt-sahlberg', 'Bengt Sahlberg');
select insert_alias  ('bengt-sahlberg', 'B. Sahlberg');
select insert_creator('bertil-wilhelmsson', 'Bertil Wilhelmsson');
select insert_alias  ('bertil-wilhelmsson', 'B. Wilhelmsson');
select insert_alias  ('bertil-wilhelmsson', 'Wilhelmsson');
select insert_alias  ('bertil-wilhelmsson', 'Bertil W-son');
select insert_creator('birgit-lundborg', 'Birgit Lundborg');
select insert_alias  ('birgit-lundborg', 'Biggan Lundborg');
select insert_alias  ('birgit-lundborg', 'B. Lundborg');
select insert_creator('bjorn-ihrstedt', 'Björn Ihrstedt');
select insert_alias  ('bjorn-ihrstedt', 'B. Ihrstedt');
select insert_alias  ('bjorn-ihrstedt', 'B Ihrstedt');
select insert_creator('carlos-cruz', 'Carlos Cruz');
select insert_alias  ('carlos-cruz', 'Carloz Cruz');
select insert_alias  ('carlos-cruz', 'C. Cruz');
select insert_alias  ('carlos-cruz', 'Cruz');
select insert_creator('claes-reimerthi', 'Claes Reimerthi');
select insert_alias  ('claes-reimerthi', 'C. Reimerthi');
select insert_alias  ('claes-reimerthi', 'Reimerthi');
select insert_creator('cesar-spadari', 'César Spadari');
select insert_alias  ('cesar-spadari', 'Cèsar Spadari');
select insert_alias  ('cesar-spadari', 'Cesàr Spadari');
select insert_alias  ('cesar-spadari', 'Cesár Spadari');
select insert_alias  ('cesar-spadari', 'Cesar Spadari');
select insert_alias  ('cesar-spadari', 'C. Spadari');
select insert_alias  ('cesar-spadari', 'Spadari');
select insert_creator('dag-frognes', 'Dag R. Frognes');
select insert_alias  ('dag-frognes', 'Dag Frognes');
select insert_creator('dai-darell', 'Dai Darell');
select insert_alias  ('dai-darell', 'Darell');
select insert_creator('dick-giordano', 'Dick Giordano');
select insert_alias  ('dick-giordano', 'Giordano');
select insert_creator('donne-avenell', 'Donne Avenell');
select insert_alias  ('donne-avenell', 'Don Avenell');
select insert_alias  ('donne-avenell', 'D. Avenell');
select insert_alias  ('donne-avenell', 'Avenell');
select insert_creator('eirik-ildahl', 'Eirik Ildahl');
select insert_alias  ('eirik-ildahl', 'Eiric Ildahl'); -- TODO Just a typo?
select insert_creator('eugenio-mattozzi', 'Eugenio Mattozzi');
select insert_alias  ('eugenio-mattozzi', 'E. Mattozzi');
select insert_creator('falco-pellerin', 'Falco Pellerin');
select insert_alias  ('falco-pellerin', 'F. Pellerin');
select insert_alias  ('falco-pellerin', 'Terje Nordberg');
select insert_creator('ferdinando-tacconi', 'Ferdinando Tacconi');
select insert_alias  ('ferdinando-tacconi', 'Fernanino Tacconi');
select insert_alias  ('ferdinando-tacconi', 'Tacconi');
select insert_creator('fred-fredericks', 'Fred Fredericks');
select insert_alias  ('fred-fredericks', 'Fredericks');
select insert_creator('georges-bessis', 'Georges Bessis');
select insert_alias  ('georges-bessis', 'Georges Bess');
select insert_alias  ('georges-bessis', 'G. Bess');
select insert_creator('germano-ferri', 'Germano Ferri');
select insert_alias  ('germano-ferri', 'Ferri');
select insert_creator('grzegorz-rosinski', 'Grzegorz Rosinski');
select insert_alias  ('grzegorz-rosinski', 'G. Rosinski');
select insert_alias  ('grzegorz-rosinski', 'Rosinski');
select insert_creator('goran-semb', 'Göran Semb');
select insert_alias  ('goran-semb', 'Semb');
select insert_creator('hans-jonsson', 'Hans Jonsson');
select insert_alias  ('hans-jonsson', 'Hans Jonson');
select insert_alias  ('hans-jonsson', 'Hasse Jonsson');
select insert_alias  ('hans-jonsson', 'H. Jonsson');
select insert_alias  ('hans-jonsson', 'H Jonsson');
select insert_creator('hans-lindahl', 'Hans Lindahl');
select insert_alias  ('hans-lindahl', 'Hasse Lindahl');
select insert_alias  ('hans-lindahl', 'H. Lindahl');
select insert_alias  ('hans-lindahl', 'Lindahl');
select insert_creator('heiner-bade', 'Heiner Bade');
select insert_alias  ('heiner-bade', 'Helmer Bade'); -- TODO Groda av redax eller mig?
select insert_alias  ('heiner-bade', 'H. Bade');
select insert_alias  ('heiner-bade', 'H Bade');
select insert_alias  ('heiner-bade', 'H. Baade');
select insert_alias  ('heiner-bade', 'Bade');
select insert_creator('henrik-brandendorff', 'Henrik Brandendorff');
select insert_alias  ('henrik-brandendorff', 'H. Brandendorff');
select insert_alias  ('henrik-brandendorff', 'Henrik Nilsson');
select insert_creator('idi-kharelli', 'Idi Kharelli');
select insert_alias  ('idi-kharelli', 'Kharelli');
select insert_creator('irene-gasc', 'Iréne Gasc');
select insert_alias  ('irene-gasc', 'Irene Gasc');
select insert_creator('ivan-boix', 'Iván Boix'); -- Son till Joan Boix
select insert_alias  ('ivan-boix', 'Ivàn Boix');
select insert_alias  ('ivan-boix', 'Ivan Boix');
select insert_creator('jaime-vallve', 'Jaime Vallvé');
select insert_alias  ('jaime-vallve', 'J. Vallvé');
select insert_alias  ('jaime-vallve', 'Vallvé');
select insert_creator('janne-lundstrom', 'Janne Lundström');
select insert_alias  ('janne-lundstrom', 'Jan Lundström');
select insert_alias  ('janne-lundstrom', 'J. Lundström');
select insert_alias  ('janne-lundstrom', 'Lundström');
select insert_creator('jean-giraud', 'Jean Giraud');
select insert_alias  ('jean-giraud', 'J. Giraud');
select insert_alias  ('jean-giraud', 'Giraud');
select insert_creator('jean-van-hamme', 'Jean Van Hamme');
select insert_alias  ('jean-van-hamme', 'J. Van Hamme');
select insert_alias  ('jean-van-hamme', 'J Van Hamme');
select insert_alias  ('jean-van-hamme', 'Van Hamme');
select insert_creator('jean-michel-charlier', 'Jean-Michel Charlier');
select insert_alias  ('jean-michel-charlier', 'J-M. Charlier');
select insert_alias  ('jean-michel-charlier', 'J-M Charlier');
select insert_alias  ('jean-michel-charlier', 'Charlier');
select insert_creator('jean-yves-mitton', 'Jean-Yves Mitton');
select insert_alias  ('jean-yves-mitton', 'J-Y Mitton');
select insert_alias  ('jean-yves-mitton', 'Mitton');
select insert_creator('karl-aage-schwartzkopf', 'Karl-Aage Schwartzkopf');
select insert_alias  ('karl-aage-schwartzkopf', 'K.-A. Schwartzkopf');
select insert_alias  ('karl-aage-schwartzkopf', 'K-A Schwartzkopf');
select insert_creator('kari-leppanen', 'Kari Leppänen');
select insert_alias  ('kari-leppanen', 'Kari T. Leppänen');
select insert_alias  ('kari-leppanen', 'Kari T Leppänen');
select insert_alias  ('kari-leppanen', 'Kari Leppänän');
select insert_alias  ('kari-leppanen', 'Kari Läppänen');
select insert_alias  ('kari-leppanen', 'Kari Läppenen');
select insert_alias  ('kari-leppanen', 'K. Leppänen');
select insert_alias  ('kari-leppanen', 'Leppänen');
select insert_creator('karin-bergh', 'Karin Bergh');
select insert_alias  ('karin-bergh', 'K. Bergh');
select insert_creator('knut-westad', 'Knut Westad');
select insert_alias  ('knut-westad', 'K. Westad');
select insert_alias  ('knut-westad', 'Westad');
select insert_creator('layla-gauraz', 'Layla Gauraz');
select insert_alias  ('layla-gauraz', 'Layla');
select insert_creator('leif-bergendorff', 'Leif Bergendorff');
select insert_alias  ('leif-bergendorff', 'L. Bergendorff');
select insert_creator('lennart-allen', 'Lennart Allen');
select insert_alias  ('lennart-allen', 'L. Allen');
select insert_creator('lennart-hartler', 'Lennart Hartler');
select insert_alias  ('lennart-hartler', 'L. Hartler');
select insert_creator('lennart-moberg', 'Lennart Moberg');
select insert_alias  ('lennart-moberg', 'L. Moberg');
select insert_alias  ('lennart-moberg', 'Moberg');
select insert_creator('marie-zackariasson', 'Marie Zackariasson');
select insert_alias  ('marie-zackariasson', 'M. Zackariasson');
select insert_creator('marian-dern', 'Marian J. Dern');
select insert_alias  ('marian-dern', 'Marian Dern');
select insert_alias  ('marian-dern', 'M. Dern');
select insert_alias  ('marian-dern', 'Dern');
select insert_creator('martin-guhl', 'Martin Guhl');
select insert_alias  ('martin-guhl', 'M. Guhl');
select insert_creator('mats-jonsson', 'Mats Jönsson');
select insert_alias  ('mats-jonsson', 'M. Jönsson');
select insert_alias  ('mats-jonsson', 'Mats Jonsson'); -- TODO Find of if this was actuall Jonsson or Jönsson
select insert_alias  ('mats-jonsson', 'M. Jonsson');
select insert_creator('matt-hollingsworth', 'Matt Hollingsworth');
select insert_alias  ('matt-hollingsworth', 'Hollingsworth');
select insert_creator('mel-keefer', 'Mel Keefer');
select insert_alias  ('mel-keefer', 'M. Keefer');
select insert_alias  ('mel-keefer', 'Keefer');
select insert_creator('michael-jaatinen', 'Michael Jaatinen');
select insert_alias  ('michael-jaatinen', 'Mikael Jaatinen');
select insert_alias  ('michael-jaatinen', 'M. Jaatinen');
select insert_creator('michael-tierres', 'Michael Tierres');
select insert_alias  ('michael-tierres', 'M. Tierres');
select insert_alias  ('michael-tierres', 'Tierres');
select insert_creator('mikael-sol', 'Mikael Sol');
select insert_alias  ('mikael-sol', 'Micke');
select insert_creator('mezieres', 'Mèziéres');
select insert_alias  ('mezieres', 'Mézières');
select insert_creator('nils-schroder', 'Nils Schröder');
select insert_alias  ('nils-schroder', 'Schröder');
select insert_creator('norman-worker', 'Norman Worker');
select insert_alias  ('norman-worker', 'N. Worker');
select insert_alias  ('norman-worker', 'Worker');
select insert_alias  ('norman-worker', 'John Bull');
select insert_alias  ('norman-worker', 'J. Bull');
select insert_creator('ola-westerberg', 'Ola Westerberg');
select insert_alias  ('ola-westerberg', 'O. Westerberg');
select insert_creator('pierre-christin', 'Pierre Christin');
select insert_alias  ('pierre-christin', 'Christin');
select insert_creator('peter-sparring', 'Peter Sparring');
select insert_alias  ('peter-sparring', 'P. Sparring');
select insert_creator('bob-kanigher', 'Robert Kanigher');
select insert_alias  ('bob-kanigher', 'Bob Kanigher');
select insert_alias  ('bob-kanigher', 'R. Kanigher');
select insert_creator('romano-felmang', 'Romano Felmang');
select insert_alias  ('romano-felmang', 'R. Felmang');
select insert_alias  ('romano-felmang', 'Felmang');
select insert_alias  ('romano-felmang', 'Roy Mann');
select insert_alias  ('romano-felmang', 'Mangiarano');
select insert_creator('rolf-gohs', 'Rolf Gohs');
select insert_alias  ('rolf-gohs', 'Gohs');
select insert_creator('scott-goodall', 'Scott Goodall');
select insert_alias  ('scott-goodall', 'S. Goodall');
select insert_alias  ('scott-goodall', 'Goodall');
select insert_creator('arthur-conan-doyle', 'Sir Arthur Conan Doyle');
select insert_alias  ('arthur-conan-doyle', 'Arthur Conan Doyle');
select insert_alias  ('arthur-conan-doyle', 'A. Conan Doyle');
select insert_alias  ('arthur-conan-doyle', 'Sir A. Conan Doyle');
select insert_creator('stefan-nagy', 'Stefan Nagy');
select insert_alias  ('stefan-nagy', 'S. Nagy');
select insert_creator('steve-ditko', 'Steve Ditko');
select insert_alias  ('steve-ditko', 'S. Ditko');
select insert_creator('sverre-arnes', 'Sverre Årnes');
select insert_alias  ('sverre-arnes', 'Årnes');
select insert_creator('terence-longstreet', 'Terence Longstreet');
select insert_alias  ('terence-longstreet', 'Terrence Longstreet');
select insert_alias  ('terence-longstreet', 'T. Longstreet');
select insert_creator('tina-stuve', 'Tina Stuve');
select insert_alias  ('tina-stuve', 'T. Stuve');
select insert_creator('todd-klein', 'Todd Klein');
select insert_alias  ('todd-klein', 'Klein');
select insert_creator('tony-depaul', 'Tony De Paul');
select insert_alias  ('tony-depaul', 'Tony DePaul');
select insert_alias  ('tony-depaul', 'Tony de Paul');
select insert_alias  ('tony-depaul', 'De Paul');
select insert_alias  ('tony-depaul', 'DePaul');
select insert_creator('tony-de-zuniga', 'Tony De Zuniga');
select insert_alias  ('tony-de-zuniga', 'Tony de Zuniga');
select insert_creator('ulf-granberg', 'Ulf Granberg');
select insert_alias  ('ulf-granberg', 'U. Granberg');
select insert_alias  ('ulf-granberg', 'Granberg');
select insert_creator('usam', 'Usam');
select insert_alias  ('usam', 'Umberto Samarini');
select insert_alias  ('usam', 'Umberto Sammarini');
select insert_creator('wally-wood', 'Wally Wood');
select insert_alias  ('wally-wood', 'Wallace Wood');
select insert_creator('william-vance', 'William Vance');
select insert_alias  ('william-vance', 'W. Vance');
select insert_alias  ('william-vance', 'Vance');
select insert_creator('yves-sente', 'Yves Sente');
select insert_alias  ('yves-sente', 'Y. Sente');
select insert_creator('zane-grey', 'Zane Grey');
select insert_alias  ('zane-grey', 'Zane Gray');
select insert_alias  ('zane-grey', 'Zane Grej');
select insert_creator('ozcan-erealp', 'Özcan Eralp');
select insert_alias  ('ozcan-erealp', 'Öscan Eralp');
select insert_alias  ('ozcan-erealp', 'Ö. Eralp');
select insert_alias  ('ozcan-erealp', 'Eralp');

drop function insert_creator;
drop function insert_alias;
