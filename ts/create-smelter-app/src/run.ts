import chalk from 'chalk';
import { resolveOptions } from './options';
import { createNodeProject } from './createNodeProject';
import { confirmPrompt } from './utils/prompts';
import { spawn } from './utils/spawn';

const SMELTER_SKILL_REPO = 'smelter-labs/skills';
const SMELTER_SKILL_NAME = 'live-composing-smelter';

export default async function() {
  const options = await resolveOptions();
  if (options.runtime.type === 'node') {
    console.log('Generating Node.js Smelter project');
    await createNodeProject(options);
  } else {
    throw new Error('Unknown project type.');
  }
  console.log();
  console.log(chalk.green('Project created successfully.'));
  console.log();

  const installSkill = await confirmPrompt(
    'Would you like to install the Smelter TypeScript SDK skill for your AI coding assistant?'
  );

  if (installSkill) {
    console.log();
    console.log('Installing Smelter skill...');
    try {
      await spawn(
        'npx',
        ['-y', 'skills', 'add', SMELTER_SKILL_REPO, '--skill', SMELTER_SKILL_NAME],
        { cwd: options.directory }
      );
      console.log(chalk.green('Smelter skill installed successfully.'));
    } catch (err: any) {
      console.log(chalk.yellow('Failed to install Smelter skill. You can install it later with:'));
      console.log(
        chalk.bold(`  npx skills add ${SMELTER_SKILL_REPO} --skill ${SMELTER_SKILL_NAME}`)
      );
    }
  }

  console.log();
  console.log(`To get started run:`);
  console.log(
    chalk.bold(
      `$ cd ${options.projectName} && ${options.packageManager} run build && node ./dist/index.js`
    )
  );
}
