package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** After the frame's mouse look, before the camera and the render. */
@Mixin(Minecraft.class)
public class MinecraftMixin {
	@Inject(method = "renderFrame", at = @At("HEAD"))
	private void lurch$frame(boolean advanceGameTime, CallbackInfo ci) {
		Lurch.frame((Minecraft) (Object) this);
	}
}
