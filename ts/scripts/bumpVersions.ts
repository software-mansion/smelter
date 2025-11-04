import path from 'path';
import fs from 'fs';

import yaml from 'yaml';
import prompt from 'prompts';
import * as semver from 'semver';

const managedPackagePaths = [
  'smelter',
  'smelter-core',
  'smelter-node',
  'smelter-browser-render',
  'smelter-web-wasm',
  'smelter-web-client',
  'create-smelter-app',
] as const;

type PackageJson = {
  version: string;
  name: string;
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
};

const VERSION_STATE_LOCATION = path.join(import.meta.dirname, '../.pre-release.json');

async function run() {
  const hasPreReleasedState = fs.existsSync(VERSION_STATE_LOCATION);
  if (hasPreReleasedState) {
    console.warn('Running in pre-release mode');
  }
  const { action } = await prompt({
    type: 'select',
    message: 'Action:',
    name: 'action',
    choices: hasPreReleasedState
      ? [
          { title: 'Bump pre-release version', value: 'pre-release' },
          { title: 'Bump regular version', value: 'bump' },
        ]
      : [
          { title: 'Bump regular version', value: 'bump' },
          { title: 'Bump pre-release version', value: 'pre-release' },
        ],
  });

  if (action === 'bump') {
    if (hasPreReleasedState) {
      await restoreVersionState();
    }
    await bumpVersions(false);
  } else if (action === 'pre-release') {
    if (!hasPreReleasedState) {
      await saveVersionState();
    }
    await bumpVersions(true);
  }
}

async function bumpVersions(isPreRelease: boolean) {
  const workspacePaths = workspacePackagePaths();
  const { selectedPaths } = await prompt({
    type: 'multiselect',
    message: 'Select package:',
    name: 'selectedPaths',
    choices: managedPackagePaths.map(pkg => ({ title: pkg, value: pkg })),
  });

  const updatedVersions: Record<string, string> = {};
  for (const selectedPath of selectedPaths) {
    const { name, version } = await promptNewVersion(selectedPath, isPreRelease);
    if (version) {
      updatedVersions[name] = version;
    }
  }

  for (const workspacePath of workspacePaths) {
    await updatePackageJsonWithVersions(workspacePath, updatedVersions, isPreRelease);
  }
}

async function updatePackageJsonWithVersions(
  packagePath: string,
  newVersionsMap: Record<string, string>,
  isPreRelease: boolean
) {
  const packageJson = await readPackageJson(packagePath);
  if (newVersionsMap[packageJson.name]) {
    packageJson.version = newVersionsMap[packageJson.name];
  }

  for (const [name, version] of Object.entries(packageJson.dependencies ?? {})) {
    const updatedVersion = await updateDependencyVersion(
      newVersionsMap,
      name,
      version,
      isPreRelease
    );
    if (updatedVersion) {
      packageJson.dependencies = packageJson.dependencies ?? {};
      packageJson.dependencies[name] = updatedVersion;
    }
  }

  for (const [name, version] of Object.entries(packageJson.devDependencies ?? {})) {
    const updatedVersion = await updateDependencyVersion(
      newVersionsMap,
      name,
      version,
      isPreRelease
    );
    if (updatedVersion) {
      packageJson.devDependencies = packageJson.devDependencies ?? {};
      packageJson.devDependencies[name] = updatedVersion;
    }
  }

  for (const [name, version] of Object.entries(packageJson.peerDependencies ?? {})) {
    const updatedVersion = isPreRelease
      ? await updateDependencyVersion(newVersionsMap, name, version, isPreRelease)
      : await updatePeerDependencyVersion(packageJson.name, newVersionsMap, name, version);
    if (updatedVersion) {
      packageJson.peerDependencies = packageJson.peerDependencies ?? {};
      packageJson.peerDependencies[name] = updatedVersion;
    }
  }

  await writePackageJson(packagePath, packageJson);
}

async function updateDependencyVersion(
  newVersionsMap: Record<string, string>,
  depName: string,
  depVersion: string,
  isPreRelease: boolean
): Promise<string | undefined> {
  const updatedVersion = newVersionsMap[depName];
  if (!updatedVersion) {
    return;
  }
  if (depVersion.startsWith('workspace:^')) {
    return isPreRelease ? `workspace:${updatedVersion}` : `workspace:^${updatedVersion}`;
  }
  if (depVersion.startsWith('workspace:~')) {
    return isPreRelease ? `workspace:${updatedVersion}` : `workspace:~${updatedVersion}`;
  }
  if (depVersion.startsWith('workspace:')) {
    return `workspace:${updatedVersion}`;
  }
  throw new Error('Unexpected version format');
}

async function updatePeerDependencyVersion(
  packageJsonPackageName: string,
  newVersionsMap: Record<string, string>,
  depName: string,
  depVersion: string
): Promise<string | undefined> {
  const updatedVersion = newVersionsMap[depName];
  if (!updatedVersion) {
    return;
  }
  let initial = '';
  if (depVersion.startsWith('workspace:^')) {
    initial = `^${updatedVersion}`;
  } else if (depVersion.startsWith('workspace:~')) {
    initial = `~${updatedVersion}`;
  } else if (depVersion.startsWith('workspace:')) {
    initial = updatedVersion;
  } else {
    throw new Error('Unexpected version format');
  }
  const { newVersion } = await prompt({
    type: 'text',
    message: `${packageJsonPackageName} peer dependency: (${depName}):`,
    name: 'newVersion',
    initial,
  });

  if (!newVersion) {
    return;
  }
  return `workspace:${newVersion}`;
}

async function promptNewVersion(
  packagePath: string,
  isPreRelease: boolean
): Promise<{ name: string; version?: string }> {
  const packageJson = await readPackageJson(packagePath);

  const alreadyPreRelease = !!semver.prerelease(packageJson.version);
  const initial = isPreRelease
    ? alreadyPreRelease
      ? semver.inc(packageJson.version, 'prerelease', 'rc')
      : semver.inc(packageJson.version, 'preminor', 'rc')
    : semver.inc(packageJson.version, 'patch');
  const { newVersion } = await prompt({
    type: 'text',
    message: `New version (${packageJson.name}):`,
    name: 'newVersion',
    initial: initial as any,
  });
  return { name: packageJson.name, version: newVersion };
}

type PersistentState = Record<
  string,
  {
    dependencies?: Record<string, string>;
    devDependencies?: Record<string, string>;
    peerDependencies?: Record<string, string>;
  }
>;

async function saveVersionState() {
  const workspacePaths = workspacePackagePaths();

  const managedPackages = new Set<string>();
  for (const packagePath of managedPackagePaths) {
    const packageJson = await readPackageJson(packagePath);
    managedPackages.add(packageJson.name);
  }

  const state: PersistentState = {};
  for (const pkgPath of workspacePaths) {
    const packageJson = await readPackageJson(pkgPath);

    const dependencies: Record<string, string> = {};
    for (const [name, version] of Object.entries(packageJson.dependencies ?? {})) {
      if (managedPackages.has(name)) {
        dependencies[name] = version;
      }
    }

    const devDependencies: Record<string, string> = {};
    for (const [name, version] of Object.entries(packageJson.devDependencies ?? {})) {
      if (managedPackages.has(name)) {
        devDependencies[name] = version;
      }
    }

    const peerDependencies: Record<string, string> = {};
    for (const [name, version] of Object.entries(packageJson.peerDependencies ?? {})) {
      if (managedPackages.has(name)) {
        peerDependencies[name] = version;
      }
    }

    state[pkgPath] = {
      dependencies,
      devDependencies,
      peerDependencies,
    };
  }

  await fs.promises.writeFile(VERSION_STATE_LOCATION, JSON.stringify(state, null, 2));
}

async function restoreVersionState(): Promise<void> {
  const versionState: PersistentState = JSON.parse(
    await fs.promises.readFile(VERSION_STATE_LOCATION, 'utf-8')
  ) as any;
  for (const [pkgPath, deps] of Object.entries(versionState)) {
    const packageJson = await readPackageJson(pkgPath);

    if (packageJson.dependencies) {
      packageJson.dependencies = { ...packageJson.dependencies, ...deps.dependencies };
    }
    if (packageJson.devDependencies) {
      packageJson.devDependencies = { ...packageJson.devDependencies, ...deps.devDependencies };
    }
    if (packageJson.peerDependencies) {
      packageJson.peerDependencies = { ...packageJson.peerDependencies, ...deps.peerDependencies };
    }

    await writePackageJson(pkgPath, packageJson);
  }
  await fs.promises.unlink(VERSION_STATE_LOCATION);
}

async function readPackageJson(packagePath: string): Promise<PackageJson> {
  const packageJsonPath = path.join(import.meta.dirname, '..', packagePath, 'package.json');
  const packageJson = await fs.promises.readFile(packageJsonPath, 'utf-8');
  return JSON.parse(packageJson);
}

async function writePackageJson(packagePath: string, packageJson: PackageJson): Promise<void> {
  const packageJsonPath = path.join(import.meta.dirname, '..', packagePath, 'package.json');
  const packageJsonText = JSON.stringify(packageJson, null, 2);
  await fs.promises.writeFile(packageJsonPath, packageJsonText + '\n', 'utf-8');
}

function workspacePackagePaths(): string[] {
  const workspaceFile = path.join(import.meta.dirname, '../pnpm-workspace.yaml');
  const fileContent = fs.readFileSync(workspaceFile, 'utf-8');
  const result = yaml.parse(fileContent);
  return result.packages;
}

void run();
