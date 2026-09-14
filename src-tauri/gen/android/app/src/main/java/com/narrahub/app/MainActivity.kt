package com.narrahub.app

import android.graphics.Rect
import android.os.Build
import android.os.Bundle
import android.view.View
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)

    // A navegação gestual do NarraHub abre puxando uma alça na borda DIREITA da tela. Com a
    // navegação por gestos do Android (10+), arrastar a partir dessa borda é o gesto de "voltar"
    // do sistema, e o sistema intercepta o toque antes de a WebView recebê-lo.
    //
    // O Android prevê exatamente este caso: o app declara uma faixa em que o gesto do sistema não
    // vale. A faixa cobre só a alça, e não a borda inteira — voltar pela borda direita continua
    // funcionando acima e abaixo dela, e pela borda esquerda sempre.
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      window.decorView.addOnLayoutChangeListener { view, left, top, right, bottom, _, _, _, _ ->
        excluirFaixaDaAlca(view, right - left, bottom - top)
      }
    }
  }

  /**
   * A faixa da alça, nas mesmas proporções do CSS (`.nh-mnav-handle`: centro a 58% da altura).
   *
   * 200 dp de altura é o máximo que o Android aceita excluir por borda; acima disso o excesso é
   * ignorado. A alça tem 112 px de CSS, e a folga cobre o polegar que não acerta o centro.
   */
  private fun excluirFaixaDaAlca(view: View, largura: Int, altura: Int) {
    if (largura <= 0 || altura <= 0) return
    val dp = resources.displayMetrics.density
    val centro = (altura * 0.58f).toInt()
    val meiaAltura = (100 * dp).toInt()
    val faixa = (48 * dp).toInt()
    view.systemGestureExclusionRects = listOf(
      Rect(largura - faixa, centro - meiaAltura, largura, centro + meiaAltura),
    )
  }
}
