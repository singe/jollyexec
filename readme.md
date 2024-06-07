# The Jolly Executioner

![image](https://github.com/singe/jollyexec/assets/1150684/ec2adbf0-05e2-4eb0-9131-bfee386a4a6b)

jollyexec is an execution proxy - it presents a configurable web server that will execute commands, and return their output. But, it has some ideas about security.

It solves some of the hassles of passing files to a command through an HTTP server by doing things like allowing you to specify if a file should be sent as stdin to a process, or whether it needs to be stuck into a temporary file and the path passed to the file.

It has a very simple "execution template" to allow you to "hardcode" the command to be executed as much as possible, to limit the client from executing things you don't want it to.

My use case was for allowing a container to execute commands on the host without providing a more general execution mechanism like ssh, which, if abused, could dissolve the container<->host security boundary.

## Configuration

Configuration is done with a simple JSON file that specifies a list of routes, where each route corresponds to a command. Take the following example:

```
{
  routes: [
  ¦ {
  ¦ ¦ path: reverse,
  ¦ ¦ command: rev,
  ¦ ¦ args: [%s]
  ]
}
```

This creates a route accessible at `/reverse` on the web server. This will execute the unix command `rev` which reverses an input file. It will accept a file in the request and pass it to the command as standard input (the `%s` in the args does that).

So this would be the equivalent of `cat file | rev` being executed on the host. The client is passing the file, but can't control the execution of `rev`.

## Execution Wrapper

The included `jollyexec_wrapper.sh` is a simple shell script that wraps this execution for you, to make it easier to invoke on the client. It's a shell script to reduce the dependencies needed in the container (i.e. avoiding the need for a full python interpreter).

It will take the route name, a variable list of parameters and paths, and construct the JSON request required, as well as convert the resulting stdout, stderr streams and exit code after execution.

It takes the following switches:

* route - what function/command you want to invoke on the jolly executioner
* -p - pass a parameter value
* -f - pass a file to be uploaded

So an example invocation for the above config file would be:

`./jollyexec_wrapper.sh reverse -f file.txt`

There's no need to use the wrapper if you don't want to, and you can just deal with the JSON directly.

## Requests and Responses

### Requests

The JSON input is made up dynamically based on the execution template you create in the configuration file. Files end up as part of a `files` dictionary and parameters in a `params` dictionary. Files need to be base64 encoded (no line wrapping). Here's an example:

```
{
  "files": [
    {"filename": "file1.txt", "data": "'"$(base64 -w0 file1.txt)"'"}
  ],
  "params": [
    "param2"
  ]
}
```

The filename can be anything and is only really used for error handling.

The resulting routes can be seen by querying the `/help` route. This will output plain text (not HTML ;) ) with a list of example curl invocations.

### Responses

Responses are simpler and are a simple JSON structure with a `stdout`, `stderr` and `exit_code`. The two output streams are also base64 encoded and will need to be decoded. The jollyexec_wrapper does this by writing to a temporary file rather than capturing the output to a variable because doing the latter seems to mess with the resulting binary (i.e. actual code would handle this better than a shell script).

Here's an example:

```
{"exit_code":0,"stderr":"Zm9vCg==","stdout":"YmFyCg=="}
```

## The Name

Comes from a Rumjacks song https://www.youtube.com/watch?v=sAmgZkPnOS4
