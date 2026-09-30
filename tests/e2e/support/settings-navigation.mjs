/** Usa o controle apresentado por cada shell, sem alterar o estado por devtools. */
export async function selecionarDispositivos(page) {
  const area = page.getByRole('combobox', { name: 'Área das configurações' });
  if (await area.isVisible()) await area.selectOption('sync');
  else await page.getByRole('button', { name: /Dispositivos/u }).click();
}
