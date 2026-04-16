import fs from 'fs';
import { Stream } from 'stream';
import { promisify } from 'util';
import path from 'path';
import type { ApiRequest, MultipartRequest } from '@swmansion/smelter-core';

const pipeline = promisify(Stream.pipeline);

export async function sendRequest(baseUrl: string | URL, request: ApiRequest): Promise<object> {
  const response = await fetch(joinUrl(baseUrl, request.route), {
    method: request.method,
    body: request.body && JSON.stringify(request.body),
    headers: {
      ...request.headers,
      'Content-Type': 'application/json',
    },
    keepalive: true,
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
    body: request.body,
    keepalive: true,
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

async function readErrorBody(response: Response): Promise<object | string> {
  const body = await response.text();
  try {
    return JSON.parse(body);
  } catch {
    return body;
  }
}
