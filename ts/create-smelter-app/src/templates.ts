import type { Template } from './applyTemplate';
import type { PackageManager } from './utils/packageManager';

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
        dirsToRemove: ['dist', 'node_modules'],
      },
    ],
    usageInstructions: defaultNodeInstructions,
  }),
};

export const NodeExpressZustandTemplate: TemplateOption = {
  title: 'Express.js + Zustand',
  description:
    'A Node.js application that streams composed video to an RTMP server. An HTTP API controls the stream layout, enables dynamic layout changes and adding MP4 files.',
  resolveTemplate: projectName => ({
    templateId: 'node-express-zustand',
    projects: [
      {
        projectName,
        dirsToRemove: ['dist', 'node_modules'],
      },
    ],
    usageInstructions: defaultNodeInstructions,
  }),
};

export const OfflineNodeMinimalTemplate: TemplateOption = {
  title: 'Generate simple MP4 file',
  description:
    'A Node.js application that generates an MP4 file, rendering a single, simple static layout.',
  resolveTemplate: projectName => ({
    templateId: 'node-offline-minimal',
    projects: [
      {
        projectName,
        dirsToRemove: ['dist', 'node_modules'],
      },
    ],
    usageInstructions: defaultNodeInstructions,
  }),
};

export const OfflineNodeShowcaseTemplate: TemplateOption = {
  title: 'Converting and combining MP4 files',
  description:
    'A Node.js application that generates an MP4 file by combining and composing multiple source MP4 files.',
  resolveTemplate: projectName => ({
    templateId: 'node-offline-minimal',
    projects: [
      {
        projectName,
        dirsToRemove: ['dist', 'node_modules'],
      },
    ],
    usageInstructions: defaultNodeInstructions,
  }),
};

export const NodeNextWebRTCTemplate: TemplateOption = {
  title: 'Streaming between Smelter and Next.js app via WebRTC',
  description:
    'A Next.js application that streams camera or screen share to the Smelter instance over WHIP. Smelter modifies the stream and broadcasts it over WHEP.',
  resolveTemplate: projectName => ({
    templateId: 'node-next-webrtc',
    projects: [
      {
        projectName,
        dir: 'server',
        dirsToRemove: ['dist', 'node_modules'],
      },
      {
        projectName,
        dir: 'client',
        dirsToRemove: ['dist', 'node_modules'],
      },
    ],
    usageInstructions: (directoryName: string, packageManager: string) => (
      'To get started run:\n' +
      `$ cd ${directoryName} && ${packageManager} run build && node ./dist/index.js`+
    ),
  }),
};
