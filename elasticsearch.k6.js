import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  stages: [
    { duration: '1m30s', target: 10 },
    { duration: '1m30s', target: 20 },
    { duration: '1m30s', target: 30 },
    { duration: '1m30s', target: 40 },
    { duration: '1m30s', target: 50 },
    { duration: '1m30s', target: 60 },
    { duration: '1m30s', target: 70 },
    { duration: '1m30s', target: 80 },
    { duration: '1m30s', target: 90 },
    { duration: '1m30s', target: 100 },

  ],
};

export default function () {
  const res = http.get('http://localhost:4444/api/elasticsearch?tag=sint');
  check(res, { 'status was 200': (r) => r.status == 200 });
  sleep(1);
}