import type { Choice } from './utils/prompts';
import { confirmPrompt, selectPrompt, textPrompt } from './utils/prompts';
import path from 'path';
import type { PackageManager } from './utils/packageManager';
import { spawn } from './utils/spawn';
import chalk from 'chalk';
import type { Template } from './applyTemplate';
import type { TemplateOption } from './templates';
import {
  NodeExpressZustandTemplate,
  NodeMinimalTemplate,
  NodeNextWebRTCTemplate,
  OfflineNodeMinimalTemplate,
  OfflineNodeShowcaseTemplate,
} from './templates';

export type ProjectOptions = {
  directory: string;
  packageManager: PackageManager;
  template: Template;
};

const packageManagers: Choice<PackageManager>[] = [
  { value: 'npm', title: 'npm' },
  { value: 'yarn', title: 'yarn' },
  { value: 'pnpm', title: 'pnpm' },
];

const templateOptions: TemplateOption[] = [
  NodeMinimalTemplate,
  NodeExpressZustandTemplate,
  NodeNextWebRTCTemplate,
  OfflineNodeMinimalTemplate,
  OfflineNodeShowcaseTemplate,
];

export async function resolveOptions(): Promise<ProjectOptions> {
  const projectName = await textPrompt('Project name: ', 'smelter-app');
  await checkFFmpeg();

  const packageManager = await resolvePackageManager();

  const template = await selectPrompt(
    'Select project template: ',
    templateOptions.map(option => ({
      title: option.title,
      description: option.description,
      value: option.resolveTemplate(projectName),
    }))
  );

  return {
    packageManager,
    template,
    directory: path.join(process.cwd(), projectName),
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
