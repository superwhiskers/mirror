

# mirror

the distributed mirroring service you never wanted (but i delivered to
you anyway)

## what

a distributed channel mirroring service for multiple services.
currently, the source code in this repository implements the discord
node, but later on this repository will hold code for other service
nodes.

mirror uses an architecture where the focal point is a rabbitmq and
scylladb cluster. with these services, communication between mirror
nodes is facilitated. at the very least, a node has to handle either
mirroring messages from a rabbitmq stream to at least one service, or
handle publishing from at least one service to a rabbitmq stream. it is
important to note that there can be multiple nodes doing the same task
for the same service as long as the nodes are capable of ensuring that
no messages are duplicated. currently, in the discord node, this is
handled by using node queues that allow nodes to signal to each other
that new tasks are being spun up so that they can sort out duplication
among themselves (𐐘).

## how

mirror can be set up relatively easily by doing the following steps:

-   fetch the latest [rabbitmq](https://hub.docker.com/_/rabbitmq) and
    [scylladb](https://hub.docker.com/r/scylladb/scylla) images from
    docker hub

-   create a scylladb container (e.g. cephalopod). this will need ports
    `9042` and `19042` published

-   create a rabbitmq container (e.g. lagomorph). this will need ports
    `5672` and `15672` published

-   write the schema (in `schema.cql`) to the scylladb node. i will not
    instruct you on how to do this. have fun

-   i was just joking, you can run `podman exec -it cephalopod cqlsh`
    and copy each of the queries to the console, assuming you've named
    your container `cephalopod` and you're using podman. if you're using
    docker you can substitute `podman` for `docker` and it should work

-   create a configuration file in the directory where you plan to run
    mirror from with the name `config.toml` and read the source code to
    understand your configuration options as there is no documentation
    for them yet

-   run the bot :3
