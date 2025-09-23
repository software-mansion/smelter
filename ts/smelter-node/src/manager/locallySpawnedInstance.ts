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
import { killProcess, spawn, spawn_ffmpeg } from '../spawn';
import { WebSocketConnection } from '../ws';
import { smelterInstanceLoggerOptions } from '../logger';
import { getSmelterStatus } from '../getSmelterStatus';

const VERSION = `v0.4.1`;

type ManagedInstanceOptions = {
  port: number;
  workingdir?: string;
  executablePath?: string;
  linkDirectories?: string[];
  enableWebRenderer?: boolean;
};

/**
 * SmelterManager that will download and spawn it's own Smelter instance locally.
 */
class LocallySpawnedInstanceManager implements SmelterManager {
  private port: number;
  private workingdir: string;
  private executablePath?: string;
  private linkDirectories?: string[];
  private wsConnection: WebSocketConnection;
  private enableWebRenderer?: boolean;
  private childSpawnPromise?: SpawnPromise;

  constructor(opts: ManagedInstanceOptions) {
    this.port = opts.port;
    this.workingdir = opts.workingdir ?? path.join(os.tmpdir(), `smelter-${uuidv4()}`);
    this.executablePath = opts.executablePath;
    this.linkDirectories = opts.linkDirectories;
    this.enableWebRenderer = opts.enableWebRenderer;
    this.wsConnection = new WebSocketConnection(`ws://127.0.0.1:${this.port}/ws`);
  }

  public static defaultManager(): LocallySpawnedInstanceManager {
    const port = process.env.SMELTER_API_PORT ? Number(process.env.SMELTER_API_PORT) : 8000;
    return new LocallySpawnedInstanceManager({
      port,
      executablePath: process.env.SMELTER_PATH,
    });
  }

  public async setupInstance(opts: SetupInstanceOptions): Promise<void> {
    // TODO: (@jbrs) Handle this nicer so linkDirectories don't depend solely on set executablePath
    const { executablePath, linkDirectories } = this.executablePath
      ? { executablePath: this.executablePath, linkDirectories: this.linkDirectories }
      : await prepareExecutable(this.enableWebRenderer);

    const { level, format } = smelterInstanceLoggerOptions();

    let downloadDir = path.join(this.workingdir, 'download');

    const arch = architecture();
    const ldEnvMac = () => {
      return {
        DYLD_LIBRARY_PATH: linkDirectories?.join(':'),
      };
    };
    const ldEnvLinux = () => {
      return { LD_LIBRARY_PATH: linkDirectories?.join(':') };
    };

    const ldPath = arch.includes('darwin') ? ldEnvMac() : ldEnvLinux();
    console.log(ldPath);

    const env = {
      SMELTER_DOWNLOAD_DIR: downloadDir,
      SMELTER_API_PORT: this.port.toString(),
      SMELTER_WEB_RENDERER_ENABLE: this.enableWebRenderer ? 'true' : 'false',
      SMELTER_AHEAD_OF_TIME_PROCESSING_ENABLE: opts.aheadOfTimeProcessing ? 'true' : 'false',
      ...process.env,
      SMELTER_LOGGER_FORMAT: format,
      SMELTER_LOGGER_LEVEL: level,
    };
    this.childSpawnPromise = spawn(executablePath, [], { env, stdio: 'inherit' });
    this.childSpawnPromise.catch(err => {
      opts.logger.error(err, 'Smelter instance failed');
      // TODO: parse structured logging from smelter and send them to this logger
      if (err.stderr) {
        console.error(err.stderr);
      }
      if (err.stdout) {
        console.error(err.stdout);
      }
    });

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
        webRendererEnable: smelterStatus.configuration.webRendererEnable,
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

async function prepareExecutable(
  enableWebRenderer?: boolean
): Promise<{ executablePath: string; linkDirectories: string[] }> {
  const { version: ffmpegVersion, linkDirectories } = await checkFFmpeg();
  console.log(ffmpegVersion);
  console.log(linkDirectories);
  const version = enableWebRenderer ? `${VERSION}-web` : VERSION;
  const downloadDir = path.join(os.homedir(), '.smelter', version, architecture());
  const readyFilePath = path.join(downloadDir, '.ready');
  const executablePath = path.join(downloadDir, 'smelter/smelter');

  if (await fs.pathExists(readyFilePath)) {
    return { executablePath, linkDirectories };
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
  return { executablePath, linkDirectories };
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

  // TODO: This will have to be set to new URL for that takes multiple releases
  // for different FFmpeg versions into account
  return `https://github.com/software-mansion/smelter/releases/download/${VERSION}/${archiveName}`;
}

async function checkFFmpeg(): Promise<{ version: string; linkDirectories: string[] }> {
  const arch = architecture();
  // Empty for linux at the moment
  const env = arch.includes('darwin') ? { DYLD_PRINT_LIBRARIES: '1' } : {};
  const re = /^ffmpeg version (\d+)\.\S+/;
  try {
    const { stdout, stderr } = await spawn_ffmpeg({
      env: { ...process.env, ...env },
    });
    const matches = stdout?.match(re);
    const version = matches?.at(1);
    if (!version) {
      throw new Error('Could not parse FFmpeg version');
    }

    const linkDirectories = parseFFmpegDependenciesDarwin(stderr ?? '');
    return { version, linkDirectories };
  } catch {
    throw new Error('FFmpeg not installed or unavailable');
  }
}

function parseFFmpegDependenciesDarwin(libraries: string): string[] {
  const ffmpegDeps = [
    'libavutil',
    'libavcodec',
    'libavformat',
    'libavdevice',
    'libavfilter',
    'libswscale',
    'libswresample',
  ];

  const librariesLines = libraries.split('\n');

  const depCheck = (line: string) => {
    const truncLine = line.slice(line.indexOf('/'));
    return ffmpegDeps.some((dep: string) => truncLine.includes(dep));
  };
  const depLibraries = librariesLines
    .filter(depCheck)
    .map((line: string) => line.slice(line.indexOf('/')))
    .filter((line: string) => line.length > 0);

  const depDirectories: string[] = [];
  for (const libPath of depLibraries) {
    const dir = path.dirname(libPath);
    if (!depDirectories.includes(dir)) {
      depDirectories.push(dir);
    }
  }
  return depDirectories;
}

export default LocallySpawnedInstanceManager;
