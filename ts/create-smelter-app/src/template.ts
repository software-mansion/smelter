import * as fs from 'fs-extra';
import path from 'path';

const TEMPLATES_ROOT = path.join(__dirname, '../templates');

export async function applyTemplate(
  destination: string,
  templateName: string,
  projectName: string
): Promise<void> {
  const templatePath = path.join(TEMPLATES_ROOT, templateName);
  await fs.copy(templatePath, destination);

  await fs.remove(path.join(destination, 'node_modules'));
  await fs.remove(path.join(destination, 'dist'));

  const packageJsonPath = path.join(destination, 'package.json');
  const packageJson = JSON.parse(await fs.readFile(packageJsonPath, 'utf8'));
  const transformedPackageJson = transformPackageJson(packageJson, projectName);
  await fs.writeFile(
    packageJsonPath,
    JSON.stringify(transformedPackageJson, null, 2) + '\n',
    'utf8'
  );
}

export function transformPackageJson(packageJson: any, projectName: string): any {
  delete packageJson?.scripts?.['start'];
  delete packageJson['private'];
  packageJson.name = projectName;
  const LABEL = 'workspace:';

  for (const dep of Object.keys((packageJson['dependencies'] as any) ?? {})) {
    const depValue: string = packageJson?.['dependencies']?.[dep];
    if (depValue && depValue.startsWith(LABEL)) {
      packageJson['dependencies'][dep] = depValue.substring(LABEL.length);
    }
  }

  for (const dep of Object.keys((packageJson['devDependencies'] as any) ?? {})) {
    const depValue: string = packageJson?.['devDependencies']?.[dep];
    if (depValue && depValue.startsWith(LABEL)) {
      packageJson['devDependencies'][dep] = depValue.substring(LABEL.length);
    }
  }

  for (const dep of Object.keys((packageJson['peerDependencies'] as any) ?? {})) {
    const depValue: string = packageJson?.['peerDependencies']?.[dep];
    if (depValue && depValue.startsWith(LABEL)) {
      packageJson['peerDependencies'][dep] = depValue.substring(LABEL.length);
    }
  }

  const devDependencies = packageJson['devDependencies'] as any;
  if (devDependencies['ts-node']) {
    delete devDependencies['ts-node'];
  }
  return packageJson;
}
