import type { ApiRequest, MultipartRequest } from '@swmansion/smelter-core';

export async function sendRequest(baseUrl: URL, request: ApiRequest): Promise<object> {
  const response = await fetch(joinUrl(baseUrl, request.route), {
    method: request.method,
    body: request.body && JSON.stringify(request.body),
    headers: {
      'Content-Type': 'application/json',
    },
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
  baseUrl: URL,
  request: MultipartRequest
): Promise<object> {
  const response = await fetch(joinUrl(baseUrl, request.route), {
    method: request.method,
    body: request.body as FormData,
  });
  if (response.status >= 400) {
    const err: any = new Error(`Request to Smelter server failed.`);
    err.response = response;
    err.body = await readErrorBody(response);
    throw err;
  }
  return (await response.json()) as object;
}

/*
 * new URL(relative, base) overrides pathname part of base URL, this function
 * appends instead
 */
export function joinUrl(base: URL | string, relative: string): URL {
  const url = new URL(base);

  if (url.pathname.endsWith('/') != relative.startsWith('/')) {
    url.pathname = url.pathname + relative;
  } else if (url.pathname.endsWith('/') && relative.startsWith('/')) {
    url.pathname = url.pathname + relative.slice(1);
  } else {
    url.pathname = url.pathname + '/' + relative;
  }

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
