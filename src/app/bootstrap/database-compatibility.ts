/** Resposta do comando `database_compatibility`, o portão que roda antes de abrir o pool. */
export interface DatabaseCompatibility {
  databaseExists: boolean;
  schemaVersion: number;
  supportedSchemaVersion: number;
  compatible: boolean;
}
