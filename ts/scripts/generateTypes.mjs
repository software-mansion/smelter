import fs from 'fs';
import * as path from 'path';
import { spawn as nodeSpawn } from 'child_process';
import { compileFromFile } from 'json-schema-to-typescript';

async function generateTypes() {
  const dirname = import.meta.dirname;
  const schemaPath = path.resolve(dirname, '../../tools/schemas/api_types.schema.json');
  const tsOutputPath = path.resolve(dirname, '../smelter/src/api.generated.ts');

  await spawn('cargo', ['run', '--package', 'tools', '--bin', 'generate_json_schema']);
  const typesTs = await compileFromFile(schemaPath, {
    additionalProperties: false,
  });
  fs.writeFileSync(tsOutputPath, typesTs);
}

function spawn(command, args) {
  const child = nodeSpawn(command, args, {
    stdio: 'inherit',
  });

  return new Promise((resolve, reject) => {
    child.on('exit', code => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`Command "${command} ${args.join(' ')}" failed with exit code ${code}.`));
      }
    });
  });
}

generateTypes();
