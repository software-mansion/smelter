import fs from 'fs';
import http from 'http';
import https from 'https';
import { Stream } from 'stream';
import { promisify } from 'util';
import path from 'path';

import fetch from 'node-fetch';
import type FormData from 'form-data';
import type { ApiRequest, MultipartRequest } from '@swmansion/smelter-core';

const pipeline = promisify(Stream.pipeline);
const httpAgent = new http.Agent({ keepAlive: true });
const httpsAgent = new https.Agent({ keepAlive: true });

export async function sendRequest(baseUrl: string | URL, request: ApiRequest): Promise<object> {
  const response = await fetch(joinUrl(baseUrl, request.route), {
    method: request.method,
    body: request.body && JSON.stringify(request.body),
    headers: {
      ...request.headers,
      'Content-Type': 'application/json',
    },
    agent: url => (url.protocol === 'http:' ? httpAgent : httpsAgent),
  });
  if (response.status >= 400) {
    const err: any = new Error(`Request to Smelter server failed.`);
    err.response = response;
    err.body = await readErrorBody(response);
    throw err;
  }
  return (await response.json()) as object;
}

export async function sendMultipartRequest(
  baseUrl: string | URL,
  request: MultipartRequest
): Promise<object> {
  const response = await fetch(joinUrl(baseUrl, request.route), {
    method: request.method,
    body: request.body as FormData,
    agent: url => (url.protocol === 'http:' ? httpAgent : httpsAgent),
    headers: request.headers,
  });
  if (response.status >= 400) {
    const err: any = new Error(`Request to Smelter server failed.`);
    err.response = response;
    err.body = await readErrorBody(response);
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

/*
 * new URL(relative, base) overrides pathname part of base URL, this function
 * appends instead
 */
export function joinUrl(base: URL | string, relative: string): URL {
  const url = new URL(base);
  url.pathname = path.join(url.pathname, relative);
  return url;
}

async function readErrorBody(response: fetch.Response): Promise<object | string> {
  const body = await response.text();
  try {
    return JSON.parse(body);
  } catch {
    return body;
  }
}
