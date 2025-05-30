export async function sleep(timeoutMs: number): Promise<void> {
  await new Promise<void>(res => {
    setTimeout(() => {
      res();
    }, timeoutMs);
  });
}

export async function retry<T>(fn: () => Promise<T>, retry: number): Promise<T> {
  let count = 0;
  while (true) {
    count += 1;
    try {
      return await fn();
    } catch (err) {
      if (count > retry) {
        throw err;
      }
    }
  }
}
