# epingm - A simple ping monitor

Simple program to monitor internet connection quality. Sends volleys of pings
and logs statistics to stdout.

This requires raw socket access, so it needs to be run as root or with the
`CAP_NET_RAW` capability.

Note: IPv6 is not supported yet.

```
Usage: epingm [OPTIONS] <TARGET>...

Arguments:
  <TARGET>...  Targets to ping

Options:
  -c, --count <COUNT>
          Number of pings to send per volley [default: 1000]
  -i, --interval <INTERVAL>
          Seconds between each ping in a volley [default: 0.01]
  -s, --size <SIZE>
          Payload size in bytes [default: 64]
      --timeout <TIMEOUT>
          Maximum number of seconds to wait for a reply [default: 10]
      --volley-interval <VOLLEY_INTERVAL>
          Seconds between each volley [default: 0]
  -f, --format <FORMAT>
          Output format [default: text] [possible values: text, csv]
  -h, --help
          Print help
```

## Usage

Ping a host:
```
epingm <host>
```

Log CSV data to a file:
```
epingm <host> -f csv > <file>
```

## Example output

```
# epingm 8.8.8.8
[2024-03-02 19:24:10] 8.8.8.8 (8.8.8.8): received: 1000/1000, lost: 0, avg: 14 ms, min: 13 ms, max: 23 ms, 50th: 14 ms, 99th: 17 ms, missing: []
[2024-03-02 19:24:20] 8.8.8.8 (8.8.8.8): received: 1000/1000, lost: 0, avg: 14 ms, min: 13 ms, max: 19 ms, 50th: 14 ms, 99th: 17 ms, missing: []
[2024-03-02 19:24:30] 8.8.8.8 (8.8.8.8): received: 1000/1000, lost: 0, avg: 14 ms, min: 13 ms, max: 19 ms, 50th: 14 ms, 99th: 16 ms, missing: []
```

```
# epingm 8.8.8.8 -f csv
time,target,ip,received,sent,lost,avg,min,max,50th,99th,missing
2024-03-02 19:26:39,8.8.8.8,8.8.8.8,1000,1000,0,14,13,20,14,16,[]
2024-03-02 19:26:49,8.8.8.8,8.8.8.8,1000,1000,0,14,13,22,14,17,[]
2024-03-02 19:26:59,8.8.8.8,8.8.8.8,1000,1000,0,14,13,19,14,17,[]
```
