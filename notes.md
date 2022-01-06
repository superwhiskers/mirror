# notes regarding design decisions

## with regard to the `ErroredMirrorTask` type in `tasks.rs` (27/9/2021)

### foreword

when thinking of how best to handle errors within a mirroring task, i
introduced exponential backoff (with a timeout), but there's more to it
than just that

### the question

initially, thinking about the best way to handle a failed mirroring task
led me to the thought that if a mirroring task were to encounter a
*fatal enough* error, it should stop itself gracefully, and the error
could be handled a few other ways from there

1.  do nothing, and just have the task be dead and start it up again or
    something

2.  the task sends an error type (through a shared mpsc channel) which
    notifies the mirror manager task about the error, allowing it to
    easily drop the mirroring task from the task `HashMap` and also
    handle it further

    it is also to note that there are further extensions to this line of
    error handling, such as

    a.  attempting to find a working rabbitmq node (since i anticipate
        this being the most likely error to occur) and restarting every
        mirroring task

    b.  gracefully killing every mirroring task, *then* killing the bot
        (antithetical to the bot's design philosophy)

    c.  nothing. we just log the error and start the mirroring task
        again

3.  we kill the entire bot process (also antithetical to the bot's
    design philosophy)

### clarification with regard to the kinds of errors passed through this channel

this channel will only be used to handle errors that are more
far-reaching than something like service ratelimiting. anything less
broad in (potential) scope should be handled within the mirroring task.

### the chosen solution

2a seems the most reasonable solution to this problem, primarily because
it can be worked to where it can avoid killing every mirroring process
in the vast majority of cases, and instead do exactly what is needed
