import type { Template } from './applyTemplate';
import type { PackageManager } from './utils/packageManager';
import chalk from 'chalk';

export interface TemplateOption {
  title: string;
  description: string;

  resolveTemplate: (projectName: string) => Template;
}

function defaultNodeInstructions(directoryName: string, packageManager: PackageManager): string {
  return (
    'To get started run:\n' +
    `$ cd ${directoryName} && ${packageManager} run build && node ./dist/index.js`
  );
}

export const NodeMinimalTemplate: TemplateOption = {
  title: 'Minimal example',
  description: 'A Node.js application that streams a simple static layout to a local RTMP server.',
  resolveTemplate: projectName => ({
    templateId: 'node-minimal',
    projects: [
      {
        projectName,
        filesToRemove: ['dist', 'node_modules'],
      },
    ],
    usageInstructions: defaultNodeInstructions,
  }),
};

export const NodeExpressZustandTemplate: TemplateOption = {
  title: 'Express.js + Zustand',
  description:
    'A Node.js application that streams composed video to a local RTMP server. An HTTP API lets you change the layout and add MP4 files at runtime.',
  resolveTemplate: projectName => ({
    templateId: 'node-express-zustand',
    projects: [
      {
        projectName,
        filesToRemove: ['dist', 'node_modules'],
      },
    ],
    usageInstructions: defaultNodeInstructions,
  }),
};

export const OfflineNodeMinimalTemplate: TemplateOption = {
  title: 'Generate an MP4 file',
  description:
    'A Node.js application that generates an MP4 file, rendering a single, simple static layout.',
  resolveTemplate: projectName => ({
    templateId: 'node-offline-minimal',
    projects: [
      {
        projectName,
        filesToRemove: ['dist', 'node_modules'],
      },
    ],
    usageInstructions: defaultNodeInstructions,
  }),
};

export const OfflineNodeShowcaseTemplate: TemplateOption = {
  title: 'Combine MP4 files',
  description:
    'A Node.js application that generates an MP4 file by combining and composing multiple source MP4 files.',
  resolveTemplate: projectName => ({
    templateId: 'node-offline-showcase',
    projects: [
      {
        projectName,
        filesToRemove: ['dist', 'node_modules'],
      },
    ],
    usageInstructions: defaultNodeInstructions,
  }),
};

export const NodeNextWebRTCTemplate: TemplateOption = {
  title: 'Next.js + WebRTC',
  description:
    'A Next.js application that streams camera or screen share to Smelter over WHIP. Smelter composes the stream and broadcasts it back over WHEP.',
  resolveTemplate: projectName => ({
    templateId: 'node-next-webrtc',
    projects: [
      {
        projectName,
        dir: 'server',
        filesToRemove: ['dist', 'node_modules'],
      },
      {
        projectName,
        dir: 'client',
        filesToRemove: ['.next', 'next-env.d.ts', 'node_modules', 'pnpm-lock.yaml'],
        packageManagerFiles: { pnpm: ['pnpm-workspace.yaml'] },
      },
    ],
    usageInstructions: (directoryName, packageManager) =>
      'To get started:\n\n' +
      '1. Start the Node.js server:\n' +
      `   $ ${chalk.bold(`cd ${directoryName}/server && ${packageManager} run build && node ./dist/index.js`)}\n\n` +
      '2. In a new terminal, start the Next.js app:\n' +
      `   $ ${chalk.bold(`cd ${directoryName}/client && ${packageManager} run dev`)}`,
  }),
};
