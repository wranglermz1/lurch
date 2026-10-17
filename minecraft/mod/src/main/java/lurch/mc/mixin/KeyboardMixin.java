package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.client.KeyboardHandler;
import net.minecraft.client.input.KeyEvent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(KeyboardHandler.class)
public class KeyboardMixin {
	@Inject(method = "keyPress", at = @At("HEAD"), cancellable = true)
	private void lurch$key(long window, int action, KeyEvent event, CallbackInfo ci) {
		if (Lurch.key(action, event)) {
			ci.cancel();
		}
	}
}
