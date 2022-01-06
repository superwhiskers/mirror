CREATE KEYSPACE IF NOT EXISTS mirrorbot
WITH replication = {'class': 'SimpleStrategy', 'replication_factor': 1}
AND durable_writes = true;

CREATE TABLE IF NOT EXISTS mirror_channels (
    id uuid,
    friendly_name text,
    private boolean,
    PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS linked_mirror_channels (
    service text,
    mirror_channel uuid,
    service_channel text,
    stream_offset smallint,
    PRIMARY KEY (service, mirror_channel, service_channel)
);

CREATE TABLE IF NOT EXISTS roles (
    mirror_channel uuid,
    user text,
    role text,
    scope text,
    PRIMARY KEY (mirror_channel, user, scope)
);

CREATE TABLE IF NOT EXISTS usercache (
    user text,
    username text,
    avatar blob,
    PRIMARY KEY (user)
);
