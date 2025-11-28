import chalk from 'chalk';
import { resolveOptions } from './options';
import { createNodeProject } from './createNodeProject';

export default async function () {
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
  console.log(`To get started run:`);
  console.log(
    chalk.bold(
      `$ cd ${options.projectName} && ${options.packageManager} run build && node ./dist/index.js`
    )
  );
}
