import fs from 'fs';
import http from 'http';
import https from 'https';
import { Stream } from 'stream';
import { promisify } from 'util';

import fetch from 'node-fetch';
import type FormData from 'form-data';
import type { ApiRequest, MultipartRequest } from '@swmansion/smelter-core';

const pipeline = promisify(Stream.pipeline);
const httpAgent = new http.Agent({ keepAlive: true });
const httpsAgent = new https.Agent({ keepAlive: true });

export async function sendRequest(baseUrl: string | URL, request: ApiRequest): Promise<object> {
  const response = await fetch(new URL(request.route, baseUrl), {
    method: request.method,
    body: request.body && JSON.stringify(request.body),
    headers: {
      'Content-Type': 'application/json',
    },
    agent: url => (url.protocol === 'http:' ? httpAgent : httpsAgent),
  });
  if (response.status >= 400) {
    const err: any = new Error(`Request to Smelter server failed.`);
    err.response = response;
    try {
      err.body = await response.json();
    } catch {
      err.body = await response.text();
    }
    throw err;
  }
  return (await response.json()) as object;
}

export async function sendMultipartRequest(
  baseUrl: string | URL,
  request: MultipartRequest
): Promise<object> {
  const response = await fetch(new URL(request.route, baseUrl), {
    method: request.method,
    body: request.body as FormData,
    agent: url => (url.protocol === 'http:' ? httpAgent : httpsAgent),
  });
  if (response.status >= 400) {
    const err: any = new Error(`Request to Smelter server failed.`);
    err.response = response;
    try {
      err.body = await response.json();
    } catch {
      err.body = await response.text();
    }
    throw err;
  }
  return (await response.json()) as object;
}

export async function download(url: string, destination: string): Promise<void> {
  const response = await fetch(url, { method: 'GET' });
  if (response.status >= 400) {
    const err: any = new Error(`Request to ${url} failed. \n${response.body}`);
    err.response = response;
    throw err;
  }
  if (response.body) {
    await pipeline(response.body, fs.createWriteStream(destination));
  } else {
    throw Error(`Response with empty body.`);
  }
}
