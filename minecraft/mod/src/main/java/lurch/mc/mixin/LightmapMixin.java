package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.client.renderer.Lightmap;
import net.minecraft.client.renderer.state.LightmapRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The lightmap's inputs each time Minecraft renders it, for lurch's arms to be lit the same. */
@Mixin(Lightmap.class)
public class LightmapMixin {
	@Inject(method = "render", at = @At("HEAD"))
	private void lurch$state(LightmapRenderState state, CallbackInfo ci) {
		Lurch.lightmap(state);
	}
}
