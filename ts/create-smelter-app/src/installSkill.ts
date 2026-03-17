import chalk from 'chalk';
import { confirmPrompt } from './utils/prompts';
import { spawn } from './utils/spawn';

const SMELTER_SKILL_REPO = 'smelter-labs/skills';
const SMELTER_SKILL_NAME = 'live-composing-smelter';

export async function promptInstallSkill(directory: string): Promise<void> {
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
        { cwd: directory }
      );
      console.log(chalk.green('Smelter skill installed successfully.'));
    } catch {
      console.log(chalk.yellow('Failed to install Smelter skill. You can install it later with:'));
      console.log(
        chalk.bold(`  npx skills add ${SMELTER_SKILL_REPO} --skill ${SMELTER_SKILL_NAME}`)
      );
    }
  }
}
