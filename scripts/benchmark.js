import http from "k6/http";

const url = __ENV.URL || 'http://localhost:3000/api/v3/topics';

export const options = {
    vus: 1, // 1 uv can use 60% CPU :joy:
    duration: '30s',
  };

export default function () {
    const params = {
        headers: {
            'Content-Type': 'application/json',
            'Accept': 'application/json',
        },
    };

    http.get(url, params);
}
