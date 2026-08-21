const required = [
  'TAURI_UPDATER_PUBLIC_KEY',
  'TAURI_SIGNING_PRIVATE_KEY',
  'TAURI_SIGNING_PRIVATE_KEY_PASSWORD',
];

const missing = required.filter((name) => !(process.env[name] || '').trim());
if (missing.length) {
  throw new Error(`Configuração do updater incompleta. Defina no GitHub: ${missing.join(', ')}.`);
}

if ((process.env.TAURI_UPDATER_PUBLIC_KEY || '').includes('PRIVATE')) {
  throw new Error('TAURI_UPDATER_PUBLIC_KEY recebeu conteúdo de chave privada. Interrompendo por segurança.');
}

console.log('Variável pública e secrets obrigatórios do updater estão configurados.');
