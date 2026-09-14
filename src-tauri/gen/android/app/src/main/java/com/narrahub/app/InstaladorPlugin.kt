package com.narrahub.app

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class AbrirInstaladorArgs {
  lateinit var caminho: String
}

/**
 * Abre o instalador do Android para um APK já baixado e conferido.
 *
 * O Rust baixa, confere o SHA-256 duas vezes e só então chama isto. Aqui fica o que só o Android
 * sabe fazer, e as últimas recusas que valem mesmo se o Rust errar:
 *
 * - o arquivo precisa estar DENTRO da pasta de cache do app — nunca um caminho qualquer;
 * - precisa existir e ter tamanho maior que zero;
 * - vai ao instalador por `content://` do FileProvider, com permissão de leitura temporária.
 *   `file://` para outro app é proibido desde o Android 7 e expõe o caminho real.
 *
 * Instalar por cima preserva o diretório de dados do app — banco, identidade, BlobStore. Quem
 * decide se aceita é o próprio Android, pela assinatura: APK assinado com outra chave é recusado
 * pelo sistema, e nada aqui tenta contornar isso.
 */
@TauriPlugin
class InstaladorPlugin(private val activity: Activity) : Plugin(activity) {

  @Command
  fun abrirInstalador(invoke: Invoke) {
    val args = invoke.parseArgs(AbrirInstaladorArgs::class.java)
    val cache = activity.cacheDir.canonicalFile
    val arquivo = File(args.caminho).canonicalFile

    if (!arquivo.path.startsWith(cache.path + File.separator)) {
      invoke.reject("O APK precisa estar na pasta de download do NarraHub.")
      return
    }
    if (!arquivo.isFile || arquivo.length() == 0L) {
      invoke.reject("O APK baixado não existe ou está vazio.")
      return
    }

    // Android 8+: instalar APK de fora da loja exige que o usuário tenha liberado o NarraHub em
    // "Instalar apps desconhecidos". Sem isso o instalador abriria e recusaria; é melhor levar a
    // pessoa direto à tela da permissão e dizer o que fazer.
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O &&
      !activity.packageManager.canRequestPackageInstalls()
    ) {
      val permissao = Intent(
        Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
        Uri.parse("package:${activity.packageName}"),
      ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
      activity.startActivity(permissao)
      invoke.resolve(JSObject().put("estado", "permissao-necessaria"))
      return
    }

    val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", arquivo)
    val instalar = Intent(Intent.ACTION_VIEW)
      .setDataAndType(uri, "application/vnd.android.package-archive")
      .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
    activity.startActivity(instalar)
    invoke.resolve(JSObject().put("estado", "instalador-aberto"))
  }
}
