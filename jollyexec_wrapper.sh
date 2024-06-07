#!/bin/sh
host="127.0.0.1"

files=""
params=""
positional_count=1

# Loop through arguments and process them
while [ "$#" -gt 0 ]; do
  case "$1" in
    -f|--file)
      if [ -n "$2" ]; then
        filename="file$positional_count.txt"
        files="${files:+$files,}{\"filename\": \"$filename\", \"data\": \"$(base64 -w0 -i "$2")\"}"
        positional_count=$((positional_count + 1))
        shift 2
      else
        echo "Error: Argument for $1 is missing" >&2
        exit 1
      fi
      ;;
    -p|--param)
      if [ -n "$2" ]; then
        params="${params:+$params,}\"$2\""
        shift 2
      else
        echo "Error: Argument for $1 is missing" >&2
        exit 1
      fi
      ;;
    -s|--stdin)
      files="${files:+$files,}{\"filename\": \"stdin.txt\", \"data\": \"$(base64 -w0)\"}"
      shift
      ;;
    -h|--help)
      echo "Jolly Executioner Wrapper - by @singe"
      echo "jollyexec_wrapper.sh <route> [-p <parameter value>] [-f <file>]"
      echo
      echo "route	- what function/command you want to invoke on the jolly executioner"
      echo "-p	- pass a parameter value"
      echo "-f	- pass a file to be uploaded"
      ;;
    *)
      route=$1
      shift
      ;;
  esac
done

json_output=$(curl -X POST http://$host:3030/$route \
     -H "Content-Type: application/json" \
     -d '{
           "files": [
             '"$files"'
           ],
           "params": [
             '"$params"'
           ]
         }' 2>/dev/null)

# Parse stdout and stderr from the JSON output
#stdout=$(echo "$json_output" | jq -r '.stdout' | base64 -d)
stdout=$(mktemp)
echo "$json_output" | jq -r '.stdout' | base64 -d > $stdout
#stderr=$(echo "$json_output" | jq -r '.stderr' | base64 -d)
stderr=$(mktemp)
echo "$json_output" | jq -r '.stderr' | base64 -d > $stderr
exit_code=$(echo "$json_output" | jq -r '.exit_code')

# Output to stdout
if [ -n "$stdout" ]; then
    #echo "$stdout"
    cat $stdout
    rm $stdout
fi

# Output to stderr
if [ -n "$stderr" ]; then
    #>&2 echo "$stderr"
    >&2 cat $stderr
    rm $stderr
fi

# Exit with the exit code from the JSON
exit $exit_code
