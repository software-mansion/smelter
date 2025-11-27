import type { Choice } from './utils/prompts';
import { confirmPrompt, selectPrompt, textPrompt } from './utils/prompts';
import path from 'path';
import type { PackageManager } from './utils/packageManager';
import { spawn } from './utils/spawn';
import chalk from 'chalk';

export type ProjectOptions = {
  projectName: string;
  directory: string;
  packageManager: PackageManager;
  runtime: BrowserOptions | NodeOptions;
};

type BrowserOptions = {
  type: 'browser';
  embeddedWasm: boolean;
  templateName: 'vite' | 'next';
};

type NodeOptions = {
  type: 'node';
  templateName: string;
};

type Runtime = 'node' | 'browser';

const packageManagers: Choice<PackageManager>[] = [
  { value: 'npm', title: 'npm' },
  { value: 'yarn', title: 'yarn' },
  { value: 'pnpm', title: 'pnpm' },
];

export async function resolveOptions(): Promise<ProjectOptions> {
  const projectName = await textPrompt('Project name: ', 'smelter-app');
  await checkFFmpeg();
  // TODO: replace
  // const runtime = await selectPrompt('Select environment:', [
  //   { value: 'node', title: 'Node.js' },
  //   { value: 'browser', title: 'Browser' },
  // ] as const);
  const runtime: Runtime = 'node' as any;

  const packageManager = await resolvePackageManager();

  let runtimeOptions: ProjectOptions['runtime'];
  if (runtime === 'browser') {
    runtimeOptions = await resolveBrowserOptions();
  } else if (runtime === 'node') {
    runtimeOptions = await resolveNodeOptions();
  } else {
    throw new Error('Unknown runtime');
  }

  return {
    runtime: runtimeOptions,
    packageManager,
    projectName,
    directory: path.join(process.cwd(), projectName),
  };
}

export async function resolveBrowserOptions(): Promise<BrowserOptions> {
  const usageType = await selectPrompt('Where do you want to run the Smelter server?', [
    { value: 'external', title: 'Run as an external instance and communicate over the network.' },
    { value: 'wasm', title: 'Embed Smelter in the browser and render using WebGL.' },
  ]);
  const templateName = await selectPrompt('Select project template:', [
    { value: 'vite', title: 'Vite + React' },
    { value: 'next', title: 'Next.js' },
  ] as const);

  return {
    type: 'browser',
    embeddedWasm: usageType === 'wasm',
    templateName,
  };
}

export async function resolveNodeOptions(): Promise<NodeOptions> {
  const templateName = await selectPrompt('Select project template: ', [
    {
      title: 'Minimal example',
      description:
        'A Node.js application that streams a simple static layout to a local RTMP server.',
      value: 'node-minimal',
    },
    {
      title: 'Express.js + Zustand',
      description:
        'A Node.js application that streams composed video to an RTMP server. An HTTP API controls the stream layout, enables dynamic layout changes and adding MP4 files.',
      value: 'node-express-zustand',
    },
    {
      title: 'Generate simple MP4 file',
      description:
        'A Node.js application that generates an MP4 file, rendering a single, simple static layout.',
      value: 'node-offline-minimal',
    },
    {
      title: 'Converting and combining MP4 files',
      description:
        'A Node.js application that generates an MP4 file by combining and composing multiple source MP4 files.',
      value: 'node-offline-showcase',
    },
  ] as const);
  return {
    type: 'node',
    templateName,
  };
}

export async function checkFFmpeg(): Promise<void> {
  try {
    await spawn('ffplay', ['-version'], { stdio: 'pipe' });
    await spawn('ffmpeg', ['-version'], { stdio: 'pipe' });
  } catch (err: any) {
    if (err.stderr) {
      console.log(chalk.red(err.stderr));
    } else {
      console.log(chalk.red(err.message));
    }
    console.log();
    console.log(
      chalk.yellow(
        `Failed to run FFmpeg command. Smelter requires FFmpeg to work and generated starter project will use "ffplay" to show the Smelter output stream.`
      )
    );
    console.log(chalk.yellow(`Please install it before continuing.`));
    if (process.platform === 'darwin') {
      console.log(chalk.yellow(`Run "${chalk.bold('brew install ffmpeg')}" to install it.`));
    }

    if (!(await confirmPrompt('Do you want to continue regardless?'))) {
      console.error('Aboring ...');
      process.exit(1);
    }
  }
}

export async function resolvePackageManager(): Promise<PackageManager> {
  const nodeUserAgent = process.env.npm_config_user_agent;
  if (nodeUserAgent?.startsWith('pnpm')) {
    return 'pnpm';
  }
  if (nodeUserAgent?.startsWith('yarn')) {
    return 'yarn';
  }

  return await selectPrompt('Select package manager: ', packageManagers);
}
