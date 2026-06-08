import fs from 'fs/promises';
import path from 'path';
import type { PackageManager } from './utils/packageManager';

const TEMPLATES_ROOT = path.join(__dirname, '../templates');

export type TemplateProject = {
  projectName: string;
  // Relative from template root, defaults to root dir
  dir?: string;
  // Relative from root from root of the project
  filesToRemove?: string[];
  // Files (relative to the project root) that are only relevant for a specific
  // package manager. They are removed when a different package manager is selected.
  packageManagerFiles?: Partial<Record<PackageManager, string[]>>;
};

export type Template = {
  templateId: string;
  projects: TemplateProject[];
  usageInstructions: (directoryName: string, packageManage: PackageManager) => string;
};

export async function applyTemplate(
  template: Template,
  destination: string,
  packageManager: PackageManager
): Promise<void> {
  const templatePath = path.join(TEMPLATES_ROOT, template.templateId);
  await fs.cp(templatePath, destination, { recursive: true });

  for (const project of template.projects) {
    const projectDir = path.join(destination, project.dir ?? '.');
    for (const dirToRemove of project.filesToRemove ?? []) {
      await fs.rm(path.join(projectDir, dirToRemove), { recursive: true, force: true });
    }

    for (const [pm, files] of Object.entries(project.packageManagerFiles ?? {})) {
      if (pm === packageManager) {
        continue;
      }
      for (const file of files) {
        await fs.rm(path.join(projectDir, file), { recursive: true, force: true });
      }
    }

    const packageJsonPath = path.join(projectDir, 'package.json');
    const packageJson = JSON.parse(await fs.readFile(packageJsonPath, 'utf8'));
    const transformedPackageJson = transformPackageJson(packageJson, project.projectName);
    await fs.writeFile(
      packageJsonPath,
      JSON.stringify(transformedPackageJson, null, 2) + '\n',
      'utf8'
    );
  }
}

export function transformPackageJson(packageJson: any, projectName: string): any {
  delete packageJson?.scripts?.['start'];
  delete packageJson?.scripts?.['lint'];
  if (packageJson?.scripts?.['_lint']) {
    packageJson.scripts['lint'] = packageJson?.scripts?.['_lint'];
    delete packageJson?.scripts?.['_lint'];
  }
  if (packageJson?.scripts?.['_start']) {
    packageJson.scripts['start'] = packageJson?.scripts?.['_start'];
    delete packageJson?.scripts?.['_start'];
  }

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
