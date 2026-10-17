package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.client.MouseHandler;
import net.minecraft.client.input.MouseButtonInfo;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(MouseHandler.class)
public class MouseMixin {
	@Inject(method = "onButton", at = @At("HEAD"), cancellable = true)
	private void lurch$button(long window, MouseButtonInfo info, int action, CallbackInfo ci) {
		if (Lurch.button(info.button(), action)) {
			ci.cancel();
		}
	}

	@Inject(method = "onScroll", at = @At("HEAD"), cancellable = true)
	private void lurch$scroll(long window, double x, double y, CallbackInfo ci) {
		if (Lurch.scroll(y)) {
			ci.cancel();
		}
	}

	@Inject(method = "onMove", at = @At("HEAD"), cancellable = true)
	private void lurch$move(long window, double x, double y, double dx, double dy, CallbackInfo ci) {
		if (Lurch.move(dx, dy)) {
			ci.cancel();
		}
	}
}
