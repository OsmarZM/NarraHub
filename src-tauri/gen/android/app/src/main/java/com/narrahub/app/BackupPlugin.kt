package com.narrahub.app

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.provider.DocumentsContract
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File
import java.security.MessageDigest

@InvokeArg
class WriteBackupArgs {
  lateinit var destination: String
  lateinit var source: String
  lateinit var name: String
  lateinit var sha256: String
}

@InvokeArg
class PruneBackupArgs {
  lateinit var destination: String
  lateinit var prefix: String
}

/** SAF: acesso somente à árvore escolhida, com permissão persistente e leitura de conferência. */
@TauriPlugin
class BackupPlugin(private val activity: Activity) : Plugin(activity) {
  private val resolver get() = activity.contentResolver
  private val managedName = Regex("NarraHub-auto-[0-9a-f]{32}-[0-9]{14}-[0-9a-f]{32}\\.narrahub-backup")

  @Command
  fun pickDirectory(invoke: Invoke) {
    val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).addFlags(
      Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
        Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
    )
    startActivityForResult(invoke, intent, "directoryResult")
  }

  @ActivityCallback
  fun directoryResult(invoke: Invoke, result: ActivityResult) {
    if (result.resultCode == Activity.RESULT_CANCELED) {
      invoke.resolve(JSObject().put("destination", org.json.JSONObject.NULL))
      return
    }
    try {
      val uri = result.data?.data ?: throw IllegalStateException("Nenhuma pasta foi selecionada.")
      if (result.resultCode != Activity.RESULT_OK) throw IllegalStateException("Não foi possível selecionar a pasta.")
      val flags = result.data!!.flags and
        (Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
      resolver.takePersistableUriPermission(uri, flags)
      requireTree(uri.toString())
      invoke.resolve(JSObject().put("destination", uri.toString()))
    } catch (error: Exception) { invoke.reject(error.message ?: "Não foi possível manter a permissão da pasta.") }
  }

  private fun requireTree(value: String): Uri {
    val uri = Uri.parse(value)
    require(uri.scheme == "content" && DocumentsContract.isTreeUri(uri)) { "Selecione uma pasta pelo Android." }
    require(resolver.persistedUriPermissions.any { it.uri == uri && it.isReadPermission && it.isWritePermission }) {
      "Permissão da pasta revogada. Escolha o destino novamente."
    }
    return uri
  }

  private fun digest(uri: Uri): String {
    val hash = MessageDigest.getInstance("SHA-256")
    val buffer = ByteArray(65536)
    resolver.openInputStream(uri)?.use { input ->
      while (true) {
        val count = input.read(buffer)
        if (count < 0) break
        hash.update(buffer, 0, count)
      }
    } ?: throw IllegalStateException("O provedor não permite conferir a cópia.")
    return hash.digest().joinToString("") { "%02x".format(it.toInt() and 255) }
  }

  @Command
  fun writeDirectory(invoke: Invoke) {
    val args = invoke.parseArgs(WriteBackupArgs::class.java)
    Thread {
      var created: Uri? = null
      try {
        val tree = requireTree(args.destination)
        require(managedName.matches(args.name)) { "Nome de backup inválido." }
        val source = File(args.source).canonicalFile
        val privateRoot = File(activity.applicationInfo.dataDir).canonicalFile
        require(source.isFile && source.name == "archive.narrahub-backup" &&
          source.parentFile?.name?.startsWith(".tmp-portable-") == true && source.parentFile?.parentFile == privateRoot) {
          "A origem precisa ser um pacote preparado pelo NarraHub."
        }
        val parent = DocumentsContract.buildDocumentUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        created = DocumentsContract.createDocument(resolver, parent, "application/octet-stream", ".${args.name}.partial")
          ?: throw IllegalStateException("O provedor recusou criar o arquivo.")
        resolver.openOutputStream(created!!, "wt")?.use { output ->
          source.inputStream().use { it.copyTo(output, 65536) }
          output.flush()
        } ?: throw IllegalStateException("O provedor recusou gravar o arquivo.")
        require(digest(created!!) == args.sha256) { "A cópia externa divergiu do pacote." }
        created = DocumentsContract.renameDocument(resolver, created!!, args.name)
          ?: throw IllegalStateException("O provedor não permite concluir o arquivo.")
        require(digest(created!!) == args.sha256) { "O arquivo final divergiu do pacote." }
        invoke.resolve(JSObject().put("uri", created.toString()))
      } catch (error: Exception) {
        created?.let { try { DocumentsContract.deleteDocument(resolver, it) } catch (_: Exception) {} }
        invoke.reject(error.message ?: "Não foi possível salvar a cópia externa.")
      }
    }.start()
  }

  @Command
  fun pruneDirectory(invoke: Invoke) {
    val args = invoke.parseArgs(PruneBackupArgs::class.java)
    Thread {
      try {
        require(Regex("NarraHub-auto-[0-9a-f]{32}-").matches(args.prefix)) { "Identificador de retenção inválido." }
        val tree = requireTree(args.destination)
        val children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, DocumentsContract.getTreeDocumentId(tree))
        val columns = arrayOf(DocumentsContract.Document.COLUMN_DOCUMENT_ID,
          DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE)
        val files = mutableListOf<Pair<String, Uri>>()
        resolver.query(children, columns, null, null, null)?.use { cursor ->
          while (cursor.moveToNext()) {
            val name = cursor.getString(1)
            if (name.startsWith(args.prefix) && managedName.matches(name) &&
              cursor.getString(2) != DocumentsContract.Document.MIME_TYPE_DIR) {
              files.add(name to DocumentsContract.buildDocumentUriUsingTree(tree, cursor.getString(0)))
            }
          }
        } ?: throw IllegalStateException("Não foi possível listar a pasta para retenção.")
        files.sortedBy { it.first }.take((files.size - 5).coerceAtLeast(0)).forEach {
          check(DocumentsContract.deleteDocument(resolver, it.second)) { "Retenção pendente; a nova cópia foi salva." }
        }
        invoke.resolve(JSObject())
      } catch (error: Exception) { invoke.reject(error.message ?: "Retenção pendente; a nova cópia foi salva.") }
    }.start()
  }
}
