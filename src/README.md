# 基于Rust语言 的epoll实例

多谢老哥的[博客](https://zupzup.org/epoll-with-rust/), 依样画葫芦写的epoll实例。


## ab test
```
 ab -c 100 -n 100000 http://localhost:8000/
This is ApacheBench, Version 2.3 <$Revision: 1843412 $>
Copyright 1996 Adam Twiss, Zeus Technology Ltd, http://www.zeustech.net/
Licensed to The Apache Software Foundation, http://www.apache.org/

Benchmarking localhost (be patient)
Completed 10000 requests
Completed 20000 requests
Completed 30000 requests
Completed 40000 requests
Completed 50000 requests
Completed 60000 requests
Completed 70000 requests
Completed 80000 requests
Completed 90000 requests
Completed 100000 requests
Finished 100000 requests


Server Software:        
Server Hostname:        localhost
Server Port:            8000

Document Path:          /
Document Length:        5 bytes

Concurrency Level:      100
Time taken for tests:   53.465 seconds
Complete requests:      100000
Failed requests:        0
Total transferred:      6400000 bytes
HTML transferred:       500000 bytes
Requests per second:    1870.39 [#/sec] (mean)
Time per request:       53.465 [ms] (mean)
Time per request:       0.535 [ms] (mean, across all concurrent requests)
Transfer rate:          116.90 [Kbytes/sec] received

Connection Times (ms)
              min  mean[+/-sd] median   max
Connect:        0    0   0.1      0       4
Processing:     1   53 135.5     16     829
Waiting:        0   53 135.2     16     829
Total:          3   53 135.5     16     829

Percentage of the requests served within a certain time (ms)
  50%     16
  66%     16
  75%     16
  80%     17
  90%     18
  95%    484
  98%    582
  99%    636
 100%    829 (longest request)
```
