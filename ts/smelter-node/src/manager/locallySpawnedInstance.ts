import os from 'os';
import path from 'path';

import { v4 as uuidv4 } from 'uuid';
import * as fs from 'fs-extra';
import * as tar from 'tar';
import type {
  ApiRequest,
  MultipartRequest,
  SmelterManager,
  SetupInstanceOptions,
} from '@swmansion/smelter-core';

import { download, sendRequest, sendMultipartRequest } from '../fetch';
import { retry, sleep } from '../utils';
import type { SpawnPromise } from '../spawn';
import { killProcess, spawn } from '../spawn';
import { WebSocketConnection } from '../ws';
import { smelterInstanceLoggerOptions } from '../logger';
import { getSmelterStatus } from '../getSmelterStatus';

// TODO: This should be changed to `software-mansion/smelter` repo with proper version on ts-sdk release
const VERSION = `62d73800`;
const REPO = `smelter-labs/smelter-rc`;

// const VERSION = `v0.4.1`;
// const REPO = `software-mansion/smelter`;

export type LocallySpawnedInstanceOptions = {
  port: number;
  workingdir?: string;
  mainExecutablePath?: string;
  dependencyCheckPath?: string;
  enableWebRenderer?: boolean;
};

type ExecutablePaths = {
  mainProcess: string;
  dependencyCheck: string;
};

/**
 * SmelterManager that will download and spawn it's own Smelter instance locally.
 */
class LocallySpawnedInstanceManager implements SmelterManager {
  private port: number;
  private workingdir: string;
  private mainExecutablePath?: string;
  private dependencyCheckPath?: string;
  private wsConnection: WebSocketConnection;
  private enableWebRenderer?: boolean;
  private childSpawnPromise?: SpawnPromise;

  constructor(opts: LocallySpawnedInstanceOptions) {
    this.port = opts.port;
    this.workingdir = opts.workingdir ?? path.join(os.tmpdir(), `smelter-${uuidv4()}`);
    this.mainExecutablePath = opts.mainExecutablePath;
    this.dependencyCheckPath = opts.dependencyCheckPath;
    this.enableWebRenderer = opts.enableWebRenderer ?? false;
    this.wsConnection = new WebSocketConnection(`ws://127.0.0.1:${this.port}/ws`);
  }

  public static defaultManager(): LocallySpawnedInstanceManager {
    const port = process.env.SMELTER_API_PORT ? Number(process.env.SMELTER_API_PORT) : 8000;
    return new LocallySpawnedInstanceManager({
      port,
      mainExecutablePath: process.env.SMELTER_PATH,
    });
  }

  public async setupInstance(opts: SetupInstanceOptions): Promise<void> {
    const { mainProcess: mainProcessPath, dependencyCheck: dependencyCheckPath } = this
      .mainExecutablePath
      ? { mainProcess: this.mainExecutablePath, dependencyCheck: this.dependencyCheckPath }
      : await prepareExecutable(this.enableWebRenderer);

    const { level, format } = smelterInstanceLoggerOptions();

    let downloadDir = path.join(this.workingdir, 'download');

    const env = {
      SMELTER_DOWNLOAD_DIR: downloadDir,
      SMELTER_API_PORT: this.port.toString(),
      SMELTER_WEB_RENDERER_ENABLE: this.enableWebRenderer ? 'true' : 'false',
      SMELTER_AHEAD_OF_TIME_PROCESSING_ENABLE: opts.aheadOfTimeProcessing ? 'true' : 'false',
      ...process.env,
      SMELTER_LOGGER_FORMAT: format,
      SMELTER_LOGGER_LEVEL: level,
    };

    const executableError = (err: any, message: string) => {
      opts.logger.error(err, message);
      // TODO: parse structured logging from smelter and send them to this logger
      if (err.stderr) {
        console.error(err.stderr);
      }
      if (err.stdout) {
        console.error(err.stdout);
      }
    };

    if (dependencyCheckPath) {
      try {
        await spawn(dependencyCheckPath, [], {});
      } catch (err) {
        executableError(err, 'Dependency check failed');
      }
    }

    this.childSpawnPromise = spawn(mainProcessPath, [], { env, stdio: 'inherit' });
    this.childSpawnPromise.catch(err => executableError(err, 'Smelter instance failed'));

    await retry(async () => {
      await sleep(500);
      let smelterStatus = await getSmelterStatus(this);

      const expectedConfig = {
        apiPort: this.port,
        downloadDir,
        webRendererEnable: this.enableWebRenderer,
        aheadOfTimeProcessing: opts.aheadOfTimeProcessing,
      };

      const actualConfig = {
        apiPort: smelterStatus.configuration.apiPort,
        downloadDir: smelterStatus.configuration.downloadRoot,
        webRendererEnable: smelterStatus.configuration.webRendererEnable ?? false,
        aheadOfTimeProcessing: smelterStatus.configuration.aheadOfTimeProcessing,
      };

      for (const [key, expected] of Object.entries(expectedConfig)) {
        const actual = actualConfig[key as keyof typeof actualConfig];
        if (actual !== expected) {
          opts.logger.warn(
            {
              key,
              expected: expected === undefined ? 'undefined' : expected,
              actual: actual === undefined ? 'undefined' : actual,
            },
            `Mismatch between instance config and SDK.`
          );
        }
      }
      return smelterStatus;
    }, 10);

    await this.wsConnection.connect(opts.logger);
  }

  public async sendRequest(request: ApiRequest): Promise<object> {
    return await sendRequest(`http://127.0.0.1:${this.port}`, request);
  }

  async sendMultipartRequest(request: MultipartRequest): Promise<object> {
    return await sendMultipartRequest(`http://127.0.0.1:${this.port}`, request);
  }
  public registerEventListener(cb: (event: object) => void): void {
    this.wsConnection.registerEventListener(cb);
  }

  public async terminate(): Promise<void> {
    await this.wsConnection.close();
    if (this.childSpawnPromise) {
      await killProcess(this.childSpawnPromise);
    }
  }
}

async function prepareExecutable(enableWebRenderer?: boolean): Promise<ExecutablePaths> {
  const version = enableWebRenderer ? `${VERSION}-web` : VERSION;
  const downloadDir = path.join(os.homedir(), '.smelter', version, architecture());
  const readyFilePath = path.join(downloadDir, '.ready');
  const executableDir = path.join(downloadDir, 'smelter');

  if (await fs.pathExists(readyFilePath)) {
    return {
      mainProcess: path.join(executableDir, 'smelter_main'),
      dependencyCheck: path.join(executableDir, 'dependency_check'),
    };
  }
  await fs.mkdirp(downloadDir);

  const tarGzPath = path.join(downloadDir, 'smelter.tar.gz');
  if (await fs.pathExists(tarGzPath)) {
    await fs.remove(tarGzPath);
  }
  await download(smelterTarGzUrl(enableWebRenderer), tarGzPath);

  await tar.x({
    file: tarGzPath,
    cwd: downloadDir,
  });
  await fs.remove(tarGzPath);

  await fs.writeFile(readyFilePath, '\n', 'utf-8');
  return {
    mainProcess: path.join(executableDir, 'smelter_main'),
    dependencyCheck: path.join(executableDir, 'dependency_check'),
  };
}

function architecture(): 'linux_aarch64' | 'linux_x86_64' | 'darwin_x86_64' | 'darwin_aarch64' {
  if (process.arch === 'x64' && process.platform === 'linux') {
    return 'linux_x86_64';
  } else if (process.arch === 'arm64' && process.platform === 'linux') {
    return 'linux_aarch64';
  } else if (process.arch === 'x64' && process.platform === 'darwin') {
    return 'darwin_x86_64';
  } else if (process.arch === 'arm64' && process.platform === 'darwin') {
    return 'darwin_aarch64';
  } else {
    throw new Error(`Unsupported platform ${process.platform} ${process.arch}`);
  }
}

function smelterTarGzUrl(withWebRenderer?: boolean): string {
  const archiveNameSuffix = withWebRenderer ? '_with_web_renderer' : '';
  const archiveName = `smelter${archiveNameSuffix}_${architecture()}.tar.gz`;
  return `https://github.com/${REPO}/releases/download/${VERSION}/${archiveName}`;
}

export default LocallySpawnedInstanceManager;
