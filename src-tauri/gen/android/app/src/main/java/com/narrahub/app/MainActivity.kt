package com.narrahub.app

import android.graphics.Color
import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.view.View
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import kotlin.math.max

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // A faixa atrás da barra de status e da barra de gestos: a mesma cor do fundo escuro do app,
    // para as barras do sistema parecerem parte da tela e não uma moldura branca.
    window.decorView.setBackgroundColor(Color.parseColor("#0B0813"))
    encaixarNasBarrasDoSistema()
  }

  /**
   * Sem zoom na interface: o NarraHub é um app, não uma página.
   *
   * Pinça e duplo toque não escalam a WebView. É uma das três camadas — as outras são a meta
   * viewport (index.html) e o `touch-action` de `src/styles/mobile.css`; ver docs/mobile/README.md.
   * Rolar, selecionar texto e os gestos da navegação não passam por aqui.
   */
  override fun onWebViewCreate(webView: WebView) {
    webView.settings.setSupportZoom(false)
    webView.settings.builtInZoomControls = false
    webView.settings.displayZoomControls = false
  }

  /**
   * O conteúdo fica ENTRE as barras do sistema, e acima do teclado.
   *
   * Com o edge-to-edge obrigatório do Android 15+, a WebView desenha por baixo da barra de status
   * e da barra de gestos, e o teclado cobre o texto que está sendo digitado. O Android entrega a
   * medida exata dessas áreas (insets); aqui elas viram padding do contêiner da WebView. O CSS
   * não precisa adivinhar `safe-area`, e o editor encolhe quando o teclado abre, como num app
   * nativo.
   */
  private fun encaixarNasBarrasDoSistema() {
    // `android.R.id.content` e o conteiner onde a WebView do Tauri e colocada; ele existe desde
    // o onCreate, antes de a WebView ser criada.
    val conteiner = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(conteiner) { view, insets ->
      val barras = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val teclado = insets.getInsets(WindowInsetsCompat.Type.ime())
      view.setPadding(barras.left, barras.top, barras.right, max(barras.bottom, teclado.bottom))
      excluirFaixaDaAlca(barras.top, max(barras.bottom, teclado.bottom))
      WindowInsetsCompat.CONSUMED
    }
    ViewCompat.requestApplyInsets(conteiner)
  }

  /**
   * A navegação gestual do NarraHub abre puxando uma alça na borda DIREITA da tela. Com a
   * navegação por gestos do Android (10+), arrastar a partir dessa borda é o gesto de "voltar"
   * do sistema, e o sistema intercepta o toque antes de a WebView recebê-lo.
   *
   * O app declara uma faixa em que o gesto do sistema não vale, só em volta da alça
   * (`.nh-mnav-handle`: centro a 58% da altura da ÁREA DO CONTEÚDO, que agora começa abaixo da
   * barra de status). 200 dp é o máximo que o Android aceita excluir por borda.
   */
  private fun excluirFaixaDaAlca(topo: Int, base: Int) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return
    val decor = window.decorView
    val largura = decor.width
    val altura = decor.height
    if (largura <= 0 || altura <= 0) {
      decor.post { excluirFaixaDaAlca(topo, base) }
      return
    }
    val dp = resources.displayMetrics.density
    val centro = topo + ((altura - topo - base) * 0.58f).toInt()
    val meiaAltura = (100 * dp).toInt()
    val faixa = (56 * dp).toInt()
    decor.systemGestureExclusionRects = listOf(
      Rect(largura - faixa, centro - meiaAltura, largura, centro + meiaAltura),
    )
  }
}
