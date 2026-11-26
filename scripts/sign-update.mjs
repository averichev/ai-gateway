import { createHash, createPrivateKey, createPublicKey, sign } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { homedir } from 'node:os';
import { join } from 'node:path';

async function signRelease(binary, buildInfoPath) {
  const identity = JSON.parse(await readFile(new URL('../release.config.json', import.meta.url), 'utf8'));
  const productId = identity.product_id;
  const jwk = identity.updates.public_key;
  const build = JSON.parse(await readFile(buildInfoPath, 'utf8'));
  if (build.product_id !== productId || build.self_update_protocol !== 1) {
    throw new Error('BuildInfo должен принадлежать AI Gateway и использовать протокол v1');
  }
  const target = /^(aarch64|x86_64)-unknown-linux-(gnu|musl)$/.exec(build.rust_target);
  const stableVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(\+[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$/;
  if (!target || !stableVersion.test(build.version)) {
    throw new Error('Нужна стабильная SemVer-сборка для Linux aarch64 или x86_64');
  }

  const arch = target[1];
  const hash = createHash('sha256');
  let size = 0;
  for await (const chunk of createReadStream(binary)) {
    if (size === 0) {
      const machine = arch === 'aarch64' ? 183 : 62;
      if (chunk.length < 20 || !chunk.subarray(0, 6).equals(Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2, 1])) ||
          chunk.readUInt16LE(18) !== machine) {
        throw new Error('Платформа исполняемого файла ELF64 расходится с BuildInfo');
      }
    }
    size += chunk.length;
    hash.update(chunk);
  }
  if (size === 0) throw new Error('Пустой исполняемый файл');

  const configDir = process.env.XDG_CONFIG_HOME || join(homedir(), '.config');
  const keyPath = process.env.GATEWAY_UPDATE_SIGNING_KEY || join(configDir, 'ai-gateway/release-signing/ed25519.pem');
  const key = createPrivateKey(await readFile(keyPath));
  if (!createPublicKey(key).equals(createPublicKey({ key: jwk, format: 'jwk' }))) {
    throw new Error('Ключ подписи не соответствует публичному ключу AI Gateway');
  }

  const thumbprint = JSON.stringify({ crv: jwk.crv, kty: jwk.kty, x: jwk.x });
  const kid = createHash('sha256').update(thumbprint).digest('base64url');
  const header = { alg: 'EdDSA', typ: 'releases-standalone+jws', kid };
  const payload = {
    schema_version: 1, application_id: productId, version: build.version,
    os: 'linux', arch, size_bytes: size, sha256: hash.digest('hex'),
  };
  const encode = (data) => Buffer.from(JSON.stringify(data)).toString('base64url');
  const input = `${encode(header)}.${encode(payload)}`;
  return `${input}.${sign(null, Buffer.from(input), key).toString('base64url')}`;
}

try {
  if (process.argv.length !== 4) throw new Error('Использование: node scripts/sign-update.mjs БИНАРНИК BUILD-INFO.json');
  console.log(await signRelease(process.argv[2], process.argv[3]));
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
