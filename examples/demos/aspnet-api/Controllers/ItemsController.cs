using Microsoft.AspNetCore.Mvc;

namespace DemoApi.Controllers;

[ApiController]
[Route("api/[controller]")]
public class ItemsController : ControllerBase
{
    private static readonly List<Item> _store = new();
    private static int _nextId = 1;

    [HttpGet]
    public IActionResult GetAll() => Ok(_store);

    [HttpGet("{id}")]
    public IActionResult GetById(int id)
    {
        var item = _store.FirstOrDefault(i => i.Id == id);
        return item is null ? NotFound() : Ok(item);
    }

    [HttpPost]
    public IActionResult Create([FromBody] ItemDto dto)
    {
        var item = new Item(_nextId++, dto.Name, dto.Description);
        _store.Add(item);
        return CreatedAtAction(nameof(GetById), new { id = item.Id }, item);
    }

    [HttpPatch("{id}")]
    public IActionResult Update(int id, [FromBody] ItemDto dto)
    {
        var item = _store.FirstOrDefault(i => i.Id == id);
        if (item is null) return NotFound();
        _store.Remove(item);
        _store.Add(item with { Name = dto.Name, Description = dto.Description });
        return Ok();
    }

    [HttpDelete("{id}")]
    public IActionResult Delete(int id)
    {
        var item = _store.FirstOrDefault(i => i.Id == id);
        if (item is null) return NotFound();
        _store.Remove(item);
        return NoContent();
    }
}

public record Item(int Id, string Name, string? Description);
public record ItemDto(string Name, string? Description);
