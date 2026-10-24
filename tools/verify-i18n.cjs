const fs = require('fs');
const path = require('path');

console.log('=== VERIFICADOR AUTOMATIZADO DE INTERNACIONALIZACIÓN (i18n) ===\n');

let hasErrors = false;

// 1. Cargar diccionarios
const esPath = path.resolve('src/lib/i18n/es-MX.json');
const enPath = path.resolve('src/lib/i18n/en.json');

const es = JSON.parse(fs.readFileSync(esPath, 'utf8'));
const en = JSON.parse(fs.readFileSync(enPath, 'utf8'));

const esKeys = Object.keys(es);
const enKeys = Object.keys(en);

console.log(`✓ Total de claves en es-MX: ${esKeys.length}`);
console.log(`✓ Total de claves en en:    ${enKeys.length}`);

// 2. Comprobar paridad 1:1 de claves
const missingInEn = esKeys.filter((k) => !(k in en));
const missingInEs = enKeys.filter((k) => !(k in es));

if (missingInEn.length > 0) {
  console.error(`\n❌ ERROR: Claves faltantes en en.json (${missingInEn.length}):`, missingInEn);
  hasErrors = true;
} else {
  console.log('✓ 100% de paridad: Ninguna clave falta en en.json.');
}

if (missingInEs.length > 0) {
  console.error(`\n❌ ERROR: Claves faltantes en es-MX.json (${missingInEs.length}):`, missingInEs);
  hasErrors = true;
} else {
  console.log('✓ 100% de paridad: Ninguna clave falta en es-MX.json.');
}

// 3. Comprobar parámetros de interpolación {variable}
console.log('\n--- VERIFICANDO PARÁMETROS INTERPOLADOS ---');
const paramRegex = /\{([a-zA-Z0-9_]+)\}/g;
let paramMismatches = 0;

for (const key of esKeys) {
  if (!(key in en)) continue;
  const esParams = [...(es[key].matchAll(paramRegex))].map((m) => m[1]).sort();
  const enParams = [...(en[key].matchAll(paramRegex))].map((m) => m[1]).sort();

  if (esParams.join(',') !== enParams.join(',')) {
    console.error(`❌ Discordancia de parámetros en la clave "${key}":`);
    console.error(`   es-MX: [${esParams.join(', ')}] -> "${es[key]}"`);
    console.error(`   en:    [${enParams.join(', ')}] -> "${en[key]}"`);
    paramMismatches++;
    hasErrors = true;
  }
}

if (paramMismatches === 0) {
  console.log('✓ Todos los parámetros {variable} coinciden exactamente entre idiomas.');
}

// 4. Escanear llamadas a t() en el código fuente
console.log('\n--- ESCANEANDO LLAMADAS A t() EN CÓDIGO FUENTE ---');
const srcFiles = [];
function walk(dir) {
  for (const item of fs.readdirSync(dir)) {
    const full = path.join(dir, item);
    if (fs.statSync(full).isDirectory()) {
      if (item !== 'node_modules' && item !== '.svelte-kit') walk(full);
    } else if (/\.(svelte|ts|tsx)$/.test(item)) {
      srcFiles.push(full);
    }
  }
}
walk('src');

const tRegex = /\bt\(\s*["']([^"']+)["']/g;
let unresolvableKeys = 0;

for (const file of srcFiles) {
  const code = fs.readFileSync(file, 'utf8');
  let m;
  while ((m = tRegex.exec(code)) !== null) {
    const key = m[1];
    // Permitir prefijos dinámicos como preset.name. o motion.state.
    if (!es[key] && !en[key] && !key.endsWith('.') && !key.includes('${')) {
      console.error(`❌ Clave no resuelta en ${file}: "${key}"`);
      unresolvableKeys++;
      hasErrors = true;
    }
  }
}

if (unresolvableKeys === 0) {
  console.log('✓ Todas las llamadas a t("...") apuntan a claves declaradas.');
}

// 5. Escaneo de posibles textos en español incrustados en UI
console.log('\n--- VERIFICANDO AUSENCIA DE TEXTOS EN ESPAÑOL HARDCODEADOS ---');
const spanishWordRegex = /(?:Punto\s+[A-Z]|Ángulo|Inclinación|Desplazamiento|Recorrido|Suavizado\s+de|Duración\s+clip|Paso\s+continuo|Parada\s+suave|Sin\s+lectura\s+reciente|Inicializando\s+diagnóstico|Espacio\s+y\s+Objetivo|Modo\s+espacial|Re-vincular\s+puntos|Punto\s+estático\s+calibrado|Ancla\s+calibrada\s+del\s+jugador|Cargar\s+recorrido|Pista\s+FX\s+\d)/;

let rawLeaks = 0;
for (const file of srcFiles) {
  if (file.includes('i18n')) continue;
  const lines = fs.readFileSync(file, 'utf8').split('\n');
  lines.forEach((line, idx) => {
    const trimmed = line.trim();
    if (trimmed.startsWith('//') || trimmed.startsWith('/*') || trimmed.startsWith('*') || trimmed.startsWith('<!--')) return;
    if (spanishWordRegex.test(trimmed)) {
      console.warn(`⚠️ Posible texto en español en ${file}:L${idx + 1}: ${trimmed.slice(0, 90)}`);
      rawLeaks++;
    }
  });
}

if (rawLeaks === 0) {
  console.log('✓ Cero filtraciones detectadas en vistas y componentes.');
} else {
  console.warn(`⚠️ Se encontraron ${rawLeaks} patrones sospechosos para revisión.`);
}

console.log('\n===============================================================');
if (hasErrors) {
  console.error('❌ LA VERIFICACIÓN DE I18N ENCONTRÓ ERRORES.');
  process.exit(1);
} else {
  console.log('🎉 AUDITORÍA DE I18N: APROBADA CON ÉXITO.');
  process.exit(0);
}
