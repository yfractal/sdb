## Capturing HTTPS plaintext without root permission
### 1. Compile `https_instrument`
`gcc -I/opt/homebrew/Cellar/openssl@1.1/1.1.1w/include -L/opt/homebrew/Cellar/openssl@1.1/1.1.1w/lib -lssl -lcrypto -shared -fPIC -o https_instrument.dylib src/https_instrument.c -lz`
### 2. Link the library through dynamic link
For example, on macOS `DYLD_INSERT_LIBRARIES=./https_instrument.dylib ruby examples/https_request.rb`
### 3. Then its plaintext will be presented
```
HTTP/1.1 200 OK
Date: Sat, 14 Sep 2024 10:10:07 GMT
Content-Type: application/json; charset=utf-8
Transfer-Encoding: chunked
Connection: keep-alive
Report-To: {"group":"heroku-nel","max_age":3600,"endpoints":[{"url":"https://nel.heroku.com/reports?ts=1724342725&sid=e11707d5-02a7-43ef-b45e-2cf4d2036f7d&s=gpx9RDwfEsdoHAUOrsyx2U2JU6n3P21e6JKJIndr92k%3D"}]}
Reporting-Endpoints: heroku-nel=https://nel.heroku.com/reports?ts=1724342725&sid=e11707d5-02a7-43ef-b45e-2cf4d2036f7d&s=gpx9RDwfEsdoHAUOrsyx2U2JU6n3P21e6JKJIndr92k%3D
Nel: {"report_to":"heroku-nel","max_age":3600,"success_fraction":0.005,"failure_fraction":0.05,"response_headers":["Via"]}
X-Powered-By: Express
X-Ratelimit-Limit: 1000
X-Ratelimit-Remaining: 999
X-Ratelimit-Reset: 1724342782
Vary: Origin, Accept-Encoding
Access-Control-Allow-Credentials: true
Cache-Control: max-age=43200
Pragma: no-cache
Expires: -1
X-Content-Type-Options: nosniff
Etag: W/"124-yiKdLzqO5gfBrJFrcdJ8Yq0LGnU"
Via: 1.1 vegur
CF-Cache-Status: HIT
Age: 7414
Server: cloudflare
CF-RAY: 8c2f95db6a4b63b8-LHR
Content-Encoding: gzip
alt-svc: h3=":443"; ma=86400


{
  "userId": 1,
  "id": 1,
  "title": "sunt aut facere repellat provident occaecati excepturi optio reprehenderit",
  "body": "quia et suscipit\nsuscipit recusandae consequuntur expedita et cum\nreprehenderit molestiae ut ut quas totam\nnostrum rerum est autem sunt rem eveniet architecto"
}
```

## NOTICE
Currently, `https_instrument` is a demo only, it may not work for some cases or even crash your program.