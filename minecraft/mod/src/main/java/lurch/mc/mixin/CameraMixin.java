package lurch.mc.mixin;

import lurch.mc.Lurch;
import net.minecraft.client.Camera;
import net.minecraft.client.DeltaTracker;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The first-person camera at lurch's eye, placed before the culling frustum is built from it. */
@Mixin(Camera.class)
public abstract class CameraMixin {
	@Shadow private boolean detached;
	@Shadow private Entity entity;

	@Shadow protected abstract void setPosition(Vec3 position);

	@Inject(method = "update", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/Camera;alignWithEntity(F)V", shift = At.Shift.AFTER))
	private void lurch$eye(DeltaTracker deltaTracker, CallbackInfo ci) {
		if (Lurch.eye != null && !detached && Lurch.moves(entity)) {
			setPosition(Lurch.eye);
		}
	}
}
