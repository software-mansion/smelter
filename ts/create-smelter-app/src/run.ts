import chalk from 'chalk';
import type { ProjectOptions } from './options';
import { resolveOptions } from './options';
import { ensureProjectDir } from './utils/workingdir';
import { applyTemplate } from './applyTemplate';
import { runPackageManagerInstall } from './utils/packageManager';
import path from 'path';

export default async function run() {
  const options = await resolveOptions();
  console.log(`Generating project in ${options.directory}`);
  await createNodeProject(options);

  console.log();
  console.log(chalk.green('Project created successfully.'));
  console.log();
  console.log(
    options.template.usageInstructions(path.basename(options.directory), options.packageManager)
  );
}

async function createNodeProject(options: ProjectOptions) {
  await ensureProjectDir(options.directory);
  await applyTemplate(options.template, options.directory);
  for (const project of options.template.projects) {
    const projectDir = path.join(options.directory, project.dir ?? '.');
    await runPackageManagerInstall(options.packageManager, projectDir);
  }
}
