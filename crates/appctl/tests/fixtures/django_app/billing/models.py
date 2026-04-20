from django.db import models


class Parcel(models.Model):
    tracking_number = models.CharField(max_length=100)
    weight_kg = models.DecimalField(max_digits=10, decimal_places=2)
    delivered = models.BooleanField(default=False)


class Customer(models.Model):
    name = models.CharField(max_length=255)
    email = models.EmailField()
    parcel = models.ForeignKey(Parcel, on_delete=models.CASCADE)
