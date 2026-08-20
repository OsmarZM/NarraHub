import { Component, input, output } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ThemeService } from '../../core/services/theme.service';

@Component({
  selector: 'app-titlebar',
  standalone: true,
  imports: [FormsModule],
  templateUrl: './titlebar.component.html',
  styleUrl: './titlebar.component.css',
})
export class TitlebarComponent {
  readonly query = input('');
  readonly workspaceMode = input(false);
  readonly queryChange = output<string>();
  readonly settingsRequested = output<void>();
  readonly homeRequested = output<void>();
  readonly minimizeRequested = output<void>();
  readonly maximizeRequested = output<void>();
  readonly closeRequested = output<void>();

  constructor(readonly theme: ThemeService) {}
}
