package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.Hud;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** lurch's viewmodel and HUD under Minecraft's chat, in place of its crosshair and hotbar. */
@Mixin(Hud.class)
public class HudMixin {
	@Inject(method = "extractRenderState", at = @At("HEAD"))
	private void lurch$overlay(GuiGraphicsExtractor g, DeltaTracker delta, CallbackInfo ci) {
		Lurch.drawOverlay(g);
	}

	@Inject(method = "extractCrosshair", at = @At("HEAD"), cancellable = true)
	private void lurch$crosshair(GuiGraphicsExtractor g, DeltaTracker delta, CallbackInfo ci) {
		if (Lurch.on()) {
			ci.cancel();
		}
	}

	@Inject(method = "extractHotbarAndDecorations", at = @At("HEAD"), cancellable = true)
	private void lurch$hotbar(GuiGraphicsExtractor g, DeltaTracker delta, CallbackInfo ci) {
		if (Lurch.on()) {
			ci.cancel();
		}
	}
}
